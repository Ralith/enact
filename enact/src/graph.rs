use rustc_hash::FxHashMap;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{Action, DuplicateAction, Seat, Session, TypeError};

/// Parses arbitrary filter graph configurations from serialized form
pub struct FilterGraphFactory {
    builders: FxHashMap<&'static str, FilterBuilder>,
}

impl FilterGraphFactory {
    /// Construct a factory that can load all standard filters
    pub fn new() -> Self {
        let mut out = Self::empty();
        out.register::<DPad>();
        out
    }

    /// Construct a factory that can't load any filters
    pub fn empty() -> Self {
        FilterGraphFactory {
            builders: FxHashMap::default(),
        }
    }

    /// Enable loading filters of type `F`
    pub fn register<F: Filter>(&mut self) {
        self.builders.insert(
            F::NAME,
            FilterBuilder {
                create_source_actions: F::create_source_actions,
                load: |session, cfg| Ok(Box::new(F::load(session, cfg)?)),
            },
        );
    }

    /// Load a serialized filter graph
    ///
    /// First, call [`register`](Self::register) to enable support for any
    /// non-standard filter-types in use.
    pub fn load(
        &self,
        session: &mut Session,
        cfg: &FilterGraphConfig,
    ) -> (FilterGraph, Vec<FilterLoadError>) {
        let mut graph = FilterGraph::new();
        graph.filters.reserve(cfg.filters.len());
        let mut builders = Vec::with_capacity(cfg.filters.len());
        let mut errors = Vec::new();
        // Create all filter source actions first so that filters can be chained arbitrarily
        for filter in &cfg.filters {
            let Some(builder) = self.builders.get(&*filter.ty) else {
                errors.push(FilterLoadError::UnknownFilter {
                    ty: filter.ty.clone(),
                });
                continue;
            };
            if let Err(e) = (builder.create_source_actions)(session, filter) {
                errors.push(e);
            }
            builders.push((builder, filter));
        }
        for (builder, filter) in builders {
            match (builder.load)(session, filter) {
                Ok(filter) => {
                    graph.filters.push(filter);
                }
                Err(e) => {
                    errors.push(e);
                }
            }
        }
        // TODO: Topological sort so chained filters get fresh data reliably
        (graph, errors)
    }
}

impl Default for FilterGraphFactory {
    /// See [`FilterGraphFactory::new`]
    fn default() -> Self {
        Self::new()
    }
}

/// A filter graph used to indirectly update actions
#[derive(Default)]
pub struct FilterGraph {
    filters: Vec<Box<dyn AnyFilter>>,
}

impl FilterGraph {
    /// Create an empty filter graph
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert the current filter graph to serialized form
    ///
    /// `session` must be the same one supplied to [`FilterGraphFactory::load`].
    pub fn save(&self, session: &Session) -> FilterGraphConfig {
        FilterGraphConfig {
            filters: self
                .filters
                .iter()
                .map(|filter| filter.save(session))
                .collect(),
        }
    }

    /// Update actions in `seat` with the filtered state
    pub fn update(&self, seat: &mut Seat) {
        for filter in &self.filters {
            filter.apply(seat);
        }
    }
}

/// Serialized form of [`FilterGraph`]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FilterGraphConfig {
    pub filters: Vec<FilterConfig>,
}

/// Serialized form of a single filter's configuration
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FilterConfig {
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub ty: String,
    pub targets: Vec<String>,
}

/// A mechanism to compute virtual inputs
pub trait Filter: Sized + 'static {
    /// A globally unique human-readable identifier for this type of filter
    ///
    /// Used in [`FilterConfig`] to identify each filter type. A single
    /// [`FilterGraphFactory`] cannot support multiple input types with the same
    /// name.
    const NAME: &str;

    fn create_source_actions(
        session: &mut Session,
        config: &FilterConfig,
    ) -> Result<(), FilterLoadError>;

    /// Construct from a [`FilterConfig`]
    fn load(session: &mut Session, config: &FilterConfig) -> Result<Self, FilterLoadError>;

    /// Convert into serializable form
    fn save(&self, session: &Session) -> FilterConfig;

    /// Generate virtual inputs in `seat`
    fn apply(&self, seat: &mut Seat);
}

trait AnyFilter {
    fn save(&self, session: &Session) -> FilterConfig;
    fn apply(&self, seat: &mut Seat);
}

impl<T: Filter> AnyFilter for T {
    fn save(&self, session: &Session) -> FilterConfig {
        Filter::save(self, session)
    }

    fn apply(&self, seat: &mut Seat) {
        Filter::apply(self, seat)
    }
}

struct FilterBuilder {
    create_source_actions:
        fn(session: &mut Session, config: &FilterConfig) -> Result<(), FilterLoadError>,

    load: fn(
        session: &mut Session,
        config: &FilterConfig,
    ) -> Result<Box<dyn AnyFilter>, FilterLoadError>,
}

/// Reasons why a filter might not be loaded
#[derive(Debug, Clone)]
pub enum FilterLoadError {
    UnknownFilter { ty: String },
    WrongOutputCount { expected: usize },
    UnknownTarget { output: String },
    DuplicateSource { name: String },
    TypeError(TypeError),
}

impl From<TypeError> for FilterLoadError {
    fn from(value: TypeError) -> Self {
        FilterLoadError::TypeError(value)
    }
}

impl From<DuplicateAction> for FilterLoadError {
    fn from(value: DuplicateAction) -> Self {
        FilterLoadError::DuplicateSource { name: value.name }
    }
}

/// Converts four directional inputs into a single [`mint::Vector2<f64>`]
///
/// Source action names are derived by suffixing `.up`/`.left`/`.down`/`.right`
/// to the target action name
pub struct DPad {
    target: Action<mint::Vector2<f64>>,

    up: Action<bool>,
    left: Action<bool>,
    down: Action<bool>,
    right: Action<bool>,
}

impl Filter for DPad {
    const NAME: &str = "dpad";

    fn create_source_actions(
        session: &mut Session,
        cfg: &FilterConfig,
    ) -> Result<(), FilterLoadError> {
        if cfg.targets.len() != 1 {
            return Err(FilterLoadError::WrongOutputCount { expected: 1 });
        }
        let o = &*cfg.targets[0];
        for dir in DPAD_DIRS {
            session.create_action::<bool>(&format!("{o}.{dir}"))?;
        }
        Ok(())
    }

    fn load(session: &mut Session, cfg: &FilterConfig) -> Result<Self, FilterLoadError> {
        let o = &*cfg.targets[0];
        let [up, left, down, right] = DPAD_DIRS
            .map(|dir| session.action::<bool>(session.action_id(&format!("{o}.{dir}")).unwrap()));
        Ok(Self {
            target: session.action(session.action_id(o).ok_or_else(|| {
                FilterLoadError::UnknownTarget {
                    output: o.to_owned(),
                }
            })?)?,
            up: up?,
            left: left?,
            down: down?,
            right: right?,
        })
    }

    fn save(&self, session: &Session) -> FilterConfig {
        FilterConfig {
            ty: Self::NAME.to_owned(),
            targets: vec![session.action_name(self.target.id()).to_owned()],
        }
    }

    fn apply(&self, seat: &mut Seat) {
        let x = seat.get(self.right).unwrap_or_default() as u64 as f64
            - seat.get(self.left).unwrap_or_default() as u64 as f64;
        let y = seat.get(self.up).unwrap_or_default() as u64 as f64
            - seat.get(self.down).unwrap_or_default() as u64 as f64;
        seat.push(self.target.id(), mint::Vector2::<f64>::from([x, y]))
            .unwrap();
    }
}

const DPAD_DIRS: [&str; 4] = ["up", "left", "down", "right"];
