#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{Action, ActionId, DuplicateAction, Seat, Session, TypeError};

/// Serialized form of a single filter's configuration
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FilterConfig {
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub ty: String,
    pub targets: Vec<String>,
}

/// A mechanism to compute virtual inputs
pub trait Filter: Sized + 'static + Clone {
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
    fn load(session: &Session, config: &FilterConfig) -> Result<Self, FilterLoadError>;

    /// Convert into serializable form
    fn save(&self, session: &Session) -> FilterConfig;

    /// Actions that this filter reads
    fn source_actions(&self) -> Vec<ActionId>;

    /// Actions that this filter writes
    fn target_actions(&self) -> Vec<ActionId>;

    /// Generate virtual inputs in `seat`
    fn apply(&self, seat: &mut Seat);
}

/// Reasons why a filter might not be loaded
#[derive(Debug, Clone)]
pub enum FilterLoadError {
    UnknownFilter {
        ty: String,
    },
    WrongOutputCount {
        expected: usize,
    },
    UnknownTarget {
        output: String,
    },
    DuplicateSource {
        name: String,
    },
    TypeError {
        filter_ty: String,
        action: String,
        error: TypeError,
    },
    Cycle,
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
#[derive(Clone)]
pub struct DPad {
    target: Action<mint::Vector2<f64>>,

    up: Action<bool>,
    left: Action<bool>,
    down: Action<bool>,
    right: Action<bool>,
}

impl DPad {
    pub fn new(
        session: &mut Session,
        target: Action<mint::Vector2<f64>>,
    ) -> Result<Self, DuplicateAction> {
        let [up, left, down, right] = DPAD_DIRS.map(|dir| {
            let o = session.action_name(target.id());
            session.create_action(&format!("{o}.{dir}"))
        });

        Ok(Self {
            target,
            up: up?,
            left: left?,
            down: down?,
            right: right?,
        })
    }

    pub fn up(&self) -> Action<bool> {
        self.up
    }
    pub fn left(&self) -> Action<bool> {
        self.left
    }
    pub fn down(&self) -> Action<bool> {
        self.down
    }
    pub fn right(&self) -> Action<bool> {
        self.right
    }
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

    fn load(session: &Session, cfg: &FilterConfig) -> Result<Self, FilterLoadError> {
        let o = &*cfg.targets[0];
        let [up, left, down, right] = DPAD_DIRS.map(|dir| {
            session
                .action::<bool>(session.action_id(&format!("{o}.{dir}")).unwrap())
                .unwrap()
        });
        Ok(Self {
            target: session
                .action(
                    session
                        .action_id(o)
                        .ok_or_else(|| FilterLoadError::UnknownTarget {
                            output: o.to_owned(),
                        })?,
                )
                .map_err(|e| FilterLoadError::TypeError {
                    filter_ty: Self::NAME.to_owned(),
                    action: o.to_owned(),
                    error: e,
                })?,
            up,
            left,
            down,
            right,
        })
    }

    fn save(&self, session: &Session) -> FilterConfig {
        FilterConfig {
            ty: Self::NAME.to_owned(),
            targets: vec![session.action_name(self.target.id()).to_owned()],
        }
    }

    fn source_actions(&self) -> Vec<ActionId> {
        [self.up, self.left, self.down, self.right]
            .map(|x| x.id())
            .into_iter()
            .collect()
    }

    fn target_actions(&self) -> Vec<ActionId> {
        vec![self.target.id()]
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
