use std::{
    any::{Any, TypeId, type_name},
    collections::VecDeque,
    fmt,
    hash::Hash,
    marker::PhantomData,
    sync::RwLock,
};

mod graph;
mod type_id_map;

use iddqd::BiHashMap;
use rustc_hash::FxHashMap;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub use graph::*;
use type_id_map::TypeIdMap;

/// A collection of [`Action`] definitions
#[derive(Default, Clone)]
pub struct Session {
    actions: BiHashMap<ActionDefinition, rustc_hash::FxBuildHasher>,
}

impl Session {
    /// Create a session with no actions
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an action with the unique identifier `name`
    ///
    /// `name` will be used to identify the action in config files and
    /// diagnostics, so it should be terse but human-readable, like a variable
    /// name. It should not be confused with localized text presented in a GUI.
    ///
    /// See [`Action`] for discussion of action design.
    pub fn create_action<T: 'static>(&mut self, name: &str) -> Result<Action<T>, DuplicateAction> {
        let id = ActionId(u32::try_from(self.actions.len()).expect("too many actions"));
        if self
            .actions
            .insert_unique(ActionDefinition {
                id,
                name: name.into(),
                ty: TypeId::of::<T>(),
                ty_name: std::any::type_name::<T>(),
            })
            .is_err()
        {
            return Err(DuplicateAction {
                name: name.to_owned(),
            });
        }
        Ok(Action {
            id,
            _marker: PhantomData,
        })
    }

    /// Get the a typed [`Action`] handle associated with an [`ActionId`]
    ///
    /// Panics if `id` was not defined in this [`Session`]
    pub fn action<T: 'static>(&self, id: ActionId) -> Result<Action<T>, TypeError> {
        let act = self.actions.get1(&id).expect("no such action");
        if act.ty != TypeId::of::<T>() {
            return Err(TypeError {
                expected: std::any::type_name::<T>(),
                actual: act.ty_name,
            });
        }
        Ok(Action {
            id,
            _marker: PhantomData,
        })
    }

    /// Get the [`ActionId`] identified by `name`, if any
    pub fn action_id(&self, name: &str) -> Option<ActionId> {
        Some(self.actions.get2(name)?.id)
    }

    /// Get the name of the action associated with an [`ActionId`]
    ///
    /// Panics if `id` was not defined in this [`Session`]
    pub fn action_name(&self, id: ActionId) -> &str {
        &self.actions.get1(&id).unwrap().name
    }

    /// Check whether an [`Input`] can be bound to the action associated with an
    /// [`ActionId`]
    ///
    /// Inputs can only be bound to actions if they produce events of the same
    /// Rust type that the action was created with.
    ///
    /// Panics if `id` was not defiend in this [`Session`]
    pub fn check_type<I: Input>(&self, id: ActionId, input: &I) -> Result<(), TypeError> {
        let act = self.actions.get1(&id).expect("no such action");
        if act.ty == input.visit_type::<GetTypeId>() {
            return Ok(());
        }
        return Err(TypeError {
            expected: input.visit_type::<GetTypeName>(),
            actual: act.ty_name,
        });
    }
}

#[derive(Debug, Clone)]
pub struct DuplicateAction {
    name: String,
}

impl fmt::Display for DuplicateAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "multiple actions named: {}", self.name)
    }
}

impl std::error::Error for DuplicateAction {}

/// A mismatch between the type of an input and an action, or between the type
/// of some data and the type described by an input.
#[derive(Debug, Clone)]
pub struct TypeError {
    expected: &'static str,
    actual: &'static str,
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "expected {}, got {}", self.expected, self.actual)
    }
}

impl std::error::Error for TypeError {}

#[derive(Clone)]
struct ActionDefinition {
    id: ActionId,
    name: String,
    ty: TypeId,
    ty_name: &'static str,
}

impl iddqd::BiHashItem for ActionDefinition {
    type K1<'a> = ActionId;

    type K2<'a> = &'a str;

    fn key1(&self) -> Self::K1<'_> {
        self.id
    }

    fn key2(&self) -> Self::K2<'_> {
        &self.name
    }

    iddqd::bi_upcast!();
}

/// Identifies a unique bindable input, such as a specific button
pub trait Input: Hash + Eq + Clone + 'static {
    /// A globally unique human-readable identifier for this type of input
    ///
    /// Used in [`Config`] to identify each input type. A single
    /// [`BindingsFactory`] cannot support multiple input types with the same
    /// name.
    const NAME: &'static str;

    /// Invoke `V::visit` on the type of data produced by `self` inputs
    fn visit_type<V: InputTypeVisitor>(&self) -> V::Output;

    /// Enumerate all inputs that `s` could represent
    ///
    /// Must return at most one input of any given type
    fn from_str(s: &str) -> Vec<Self>;

    /// Generate a human-readable string identifying this input
    ///
    /// [`from_str`](Self::from_str) on the resulting string must include a
    /// value equivalent to `self` in its result
    fn to_string(&self) -> String;
}

/// Returns `Some` iff `input` produces events of type `T`
pub fn has_type<T: 'static, I: Input>(input: &I) -> bool {
    input.visit_type::<GetTypeId>() == TypeId::of::<T>()
}

/// Helper to inspect the type of data associated with an [`Input`] via
/// [`Input::visit_type`]
pub trait InputTypeVisitor {
    type Output;
    fn visit<T: 'static>() -> Self::Output;
}

struct GetTypeId;

impl InputTypeVisitor for GetTypeId {
    type Output = TypeId;
    fn visit<T: 'static>() -> TypeId {
        TypeId::of::<T>()
    }
}

struct GetTypeName;

impl InputTypeVisitor for GetTypeName {
    type Output = &'static str;
    fn visit<T: 'static>() -> &'static str {
        type_name::<T>()
    }
}

/// Parses bindings for arbitrary input types from serialized form
#[derive(Clone)]
pub struct BindingsFactory {
    input_binding_builders: FxHashMap<
        String,
        (
            TypeId,
            fn(&Session, &SourceConfig) -> (Box<dyn AnyInputBindings>, Vec<LoadError>),
        ),
    >,
    filter_builders: FxHashMap<&'static str, FilterBuilder>,
}

impl BindingsFactory {
    /// Construct a factory with support for default filters
    ///
    /// Don't forget to call [`register_source`](Self::register_source) with all
    /// desired input sources.
    pub fn new() -> Self {
        let mut out = Self::empty();
        out.register_filter::<DPad>();
        out
    }

    /// Construct a factory with no default filters
    pub fn empty() -> Self {
        Self {
            input_binding_builders: Default::default(),
            filter_builders: Default::default(),
        }
    }

    /// Enable loading configurations that include inputs of type `I`
    pub fn register_source<I: Input>(&mut self) {
        self.input_binding_builders.insert(
            I::NAME.to_string(),
            (TypeId::of::<I>(), |session, cfg| {
                let mut bindings = FxHashMap::<I, Vec<ActionId>>::default();
                let mut errors = Vec::new();
                for (name, inputs) in &cfg.bindings {
                    let Some(action) = session.action_id(name) else {
                        errors.push(LoadError::UnknownAction { name: name.clone() });
                        continue;
                    };
                    for input_str in inputs {
                        let inputs = I::from_str(input_str);
                        if inputs.is_empty() {
                            errors.push(LoadError::UnknownInput {
                                input: input_str.clone(),
                            });
                            continue;
                        }
                        let mut expected = Vec::new();
                        let mut success = false;
                        for input in inputs {
                            if let Err(error) = session.check_type(action, &input) {
                                expected.push(error.expected);
                            } else {
                                bindings.entry(input).or_default().push(action);
                                success = true;
                                break;
                            }
                        }
                        if !success {
                            errors.push(LoadError::InputTypeError {
                                action_name: name.clone(),
                                input: input_str.clone(),
                                actual: session.actions.get1(&action).unwrap().ty_name,
                                expected,
                            })
                        }
                    }
                }
                (Box::new(InputBindings { bindings }), errors)
            }),
        );
    }

    /// Enable loading filters of type `F`
    pub fn register_filter<F: Filter>(&mut self) {
        self.filter_builders.insert(
            F::NAME,
            FilterBuilder {
                create_source_actions: F::create_source_actions,
                load: |session, cfg| Ok(Box::new(F::load(session, cfg)?)),
            },
        );
    }

    /// Load a serialized configuration
    ///
    /// Filters defined in `config` may add new actions to `session`.
    ///
    /// First, call [`register_source`](Self::register_source) to enable support for any
    /// desired input sources, and create all desired actions in the
    /// [`Session`].
    ///
    /// Malformed inputs will be recorded in the returned [`LoadError`]s, but
    /// will not terminate parsing: all well-formed bindings will be included in
    /// the resulting [`Bindings`].
    pub fn load(&self, session: &mut Session, config: &Config) -> (Bindings, Vec<LoadError>) {
        let mut bindings = Bindings::new();
        let mut errors = Vec::new();

        // Create all filter source actions first so that filters can be chained arbitrarily
        let mut filter_builders = Vec::with_capacity(config.filters.len());
        for filter in &config.filters {
            let Some(builder) = self.filter_builders.get(&*filter.ty) else {
                errors.push(
                    FilterLoadError::UnknownFilter {
                        ty: filter.ty.clone(),
                    }
                    .into(),
                );
                continue;
            };
            if let Err(e) = (builder.create_source_actions)(session, filter) {
                errors.push(e.into());
            }
            filter_builders.push((builder, filter));
        }
        for (builder, filter) in filter_builders {
            match (builder.load)(session, filter) {
                Ok(filter) => {
                    bindings.add_any_filter(filter);
                }
                Err(e) => {
                    errors.push(e.into());
                }
            }
        }

        for source in &config.sources {
            let Some((ty, builder)) = self.input_binding_builders.get(&source.ty) else {
                errors.push(LoadError::UnknownSource {
                    name: source.ty.clone(),
                });
                continue;
            };
            let (built, source_errors) = builder(session, source);
            // Future work: Merge duplicates?
            bindings.actions.insert(*ty, built);
            errors.extend(source_errors.into_iter());
        }
        (bindings, errors)
    }
}

impl Default for BindingsFactory {
    /// See [`new`](Self::new)
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Copy, Clone)]
struct FilterBuilder {
    create_source_actions:
        fn(session: &mut Session, config: &FilterConfig) -> Result<(), FilterLoadError>,

    load:
        fn(session: &Session, config: &FilterConfig) -> Result<Box<dyn AnyFilter>, FilterLoadError>,
}

trait AnyFilter {
    fn save(&self, session: &Session) -> FilterConfig;
    fn apply(&self, seat: &mut Seat);
    fn clone(&self) -> Box<dyn AnyFilter>;
    fn source_actions(&self) -> Vec<ActionId>;
    fn target_actions(&self) -> Vec<ActionId>;
}

impl<T: Filter> AnyFilter for T {
    fn save(&self, session: &Session) -> FilterConfig {
        Filter::save(self, session)
    }

    fn apply(&self, seat: &mut Seat) {
        Filter::apply(self, seat)
    }

    fn clone(&self) -> Box<dyn AnyFilter> {
        Box::new(Clone::clone(self))
    }

    fn source_actions(&self) -> Vec<ActionId> {
        Filter::source_actions(self)
    }

    fn target_actions(&self) -> Vec<ActionId> {
        Filter::target_actions(self)
    }
}

/// Reasons why soem part of a [`Config`] might not be loaded
#[derive(Debug, Clone)]
pub enum LoadError {
    /// This type of inputs did not match any type previously supplied to
    /// [`BindingsFactory::register`]
    UnknownSource {
        name: String,
    },
    /// The action name was not defined in the [`Session`]
    UnknownAction {
        name: String,
    },
    /// A specific input binding was not recognized
    UnknownInput {
        input: String,
    },
    /// A specific input binding cannot produce data of the type expected by a
    /// specific action
    InputTypeError {
        action_name: String,
        input: String,
        actual: &'static str,
        expected: Vec<&'static str>,
    },
    Filter(FilterLoadError),
}

impl From<FilterLoadError> for LoadError {
    fn from(value: FilterLoadError) -> Self {
        LoadError::Filter(value)
    }
}

/// A mapping of inputs to actions
///
/// [`Bindings`] are always defined with respect to the actions defined in a
/// specific [`Session`]. Never mix actions from multiple [`Session`]s.
#[derive(Default)]
pub struct Bindings {
    actions: TypeIdMap<Box<dyn AnyInputBindings>>,
    filters: Vec<Box<dyn AnyFilter>>,
    /// Maps actions to the index in `filters` of the filter that consumes them
    filter_source_actions: FxHashMap<ActionId, usize>,
}

impl Bindings {
    /// Create an empty set of bindings
    pub fn new() -> Self {
        Self::default()
    }

    /// Convert the current set of bindings into serializable form
    ///
    /// `session` must be the same one used to create all [`Action`]s described
    /// in these bindings.
    pub fn save(&self, session: &Session) -> Config {
        Config {
            sources: self
                .actions
                .values()
                .map(|value| value.save(session))
                .collect(),
            filters: self
                .filters
                .iter()
                .map(|filter| filter.save(session))
                .collect(),
        }
    }

    /// Add a filter to the filter graph
    pub fn add_filter<F: Filter>(&mut self, filter: F) {
        self.add_any_filter(Box::new(filter));
    }

    fn add_any_filter(&mut self, filter: Box<dyn AnyFilter>) {
        // Should we support multiple filters reading from the same action?
        self.filter_source_actions.extend(
            filter
                .source_actions()
                .into_iter()
                .map(|x| (x, self.filters.len())),
        );
        self.filters.push(filter);
    }

    /// Introduce a new binding from `input` to `action`
    ///
    /// All [`Action`]s in a set of bindings must be created from the same
    /// [`Session`].
    pub fn bind<I: Input>(
        &mut self,
        input: I,
        action: ActionId,
        session: &Session,
    ) -> Result<(), TypeError> {
        session.check_type(action, &input)?;
        let bindings = self
            .actions
            .entry(TypeId::of::<I>())
            .or_insert_with(|| Box::new(InputBindings::<I>::default()));
        let bindings = (&mut **bindings as &mut dyn Any)
            .downcast_mut::<InputBindings<I>>()
            .unwrap();
        bindings.bindings.entry(input).or_default().push(action);
        Ok(())
    }

    /// Change the state of `input` to `data` in `seat`
    ///
    /// Most applications do not need to call this directly. Instead, call the
    /// handler responsible for processing foreign events provided by the crate
    /// in which the `Input` type is defined.
    pub fn handle<I: Input, T: Clone + 'static>(
        &self,
        input: &I,
        data: T,
        seat: &mut Seat,
    ) -> Result<(), TypeError> {
        if TypeId::of::<T>() != input.visit_type::<GetTypeId>() {
            // `input` can't produce data of type `T`
            return Err(TypeError {
                expected: input.visit_type::<GetTypeName>(),
                actual: type_name::<T>(),
            });
        }
        let Some(actions) = self.actions.get(&TypeId::of::<I>()) else {
            // No bindings exist for inputs of this type
            return Ok(());
        };
        let Some(bindings) = (&**actions as &dyn Any)
            .downcast_ref::<InputBindings<I>>()
            .unwrap()
            .bindings
            .get(input)
        else {
            // No bindings exist for this specific input
            return Ok(());
        };
        for &action in bindings {
            // Guaranteed to succeed because we check types at bind time
            seat.push(action, data.clone()).unwrap();
            self.propagate(action, seat);
        }
        Ok(())
    }

    /// Update actions populated from filters dependent on `action` in `seat`
    fn propagate(&self, action: ActionId, seat: &mut Seat) {
        let mut dirty = vec![action];
        while let Some(action) = dirty.pop() {
            let Some(&filter) = self.filter_source_actions.get(&action) else {
                continue;
            };
            let filter = &self.filters[filter];
            filter.apply(seat);
            dirty.extend(filter.target_actions())
        }
    }
}

impl Clone for Bindings {
    fn clone(&self) -> Self {
        Self {
            actions: self
                .actions
                .iter()
                .map(|(&k, v)| (k, AnyInputBindings::clone(&**v)))
                .collect(),
            filters: self
                .filters
                .iter()
                .map(|f| AnyFilter::clone(&**f))
                .collect(),
            filter_source_actions: self.filter_source_actions.clone(),
        }
    }
}

trait AnyInputBindings: Any {
    fn save(&self, session: &Session) -> SourceConfig;
    fn clone(&self) -> Box<dyn AnyInputBindings>;
}

impl<I: Input> AnyInputBindings for InputBindings<I> {
    fn save(&self, session: &Session) -> SourceConfig {
        let mut bindings = FxHashMap::<String, Vec<String>>::default();
        // Transpose
        for (input, actions) in &self.bindings {
            for &action in actions {
                let name = session.action_name(action);
                if !bindings.contains_key(name) {
                    bindings.insert(name.to_owned(), Vec::new());
                }
                bindings.get_mut(name).unwrap().push(input.to_string());
            }
        }
        let mut bindings = bindings.into_iter().collect::<Vec<_>>();
        // Sort for readability
        // Future work: preserve loaded order?
        bindings.sort_unstable_by(|x, y| x.0.cmp(&y.0));
        SourceConfig {
            ty: I::NAME.to_owned(),
            bindings,
        }
    }
    fn clone(&self) -> Box<dyn AnyInputBindings> {
        Box::new(Clone::clone(self))
    }
}

struct InputBindings<I: Input> {
    bindings: FxHashMap<I, Vec<ActionId>>,
}

impl<I: Input> Clone for InputBindings<I> {
    fn clone(&self) -> Self {
        Self {
            bindings: self.bindings.clone(),
        }
    }
}

impl<I: Input> Default for InputBindings<I> {
    fn default() -> Self {
        Self {
            bindings: FxHashMap::default(),
        }
    }
}

/// Serialized form of [`Bindings`]
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Config {
    #[cfg_attr(feature = "serde", serde(default))]
    pub sources: Vec<SourceConfig>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub filters: Vec<FilterConfig>,
}

/// Subset of serialized [`Bindings`] associated with a specific input source
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SourceConfig {
    /// The [`Input::NAME`] of the input source that should interpret these
    /// bindings
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub ty: String,
    /// Maps action names to inputs from this input source
    #[cfg_attr(feature = "serde", serde(with = "tuple_vec_map"))]
    pub bindings: Vec<(String, Vec<String>)>,
}

/// Represents the current state and recent history of any active [`Action`]s
///
/// Applications may call [`poll`](Self::poll) to observe changes to action
/// state, or [`get`](Self::get) to sample the latest state. In either case,
/// [`flush`](Self::flush) must be called regularly to discard any records of
/// changes to action state which were not consumed by a [`poll`](Self::poll)
/// call.
#[derive(Default)]
pub struct Seat {
    state: Vec<Option<Box<RwLock<dyn AnyState>>>>,
}

impl Seat {
    /// Create a seat with no action state
    pub fn new() -> Self {
        Self::default()
    }

    /// Consume the next state change affecting `action`, if any
    pub fn poll<T: 'static>(&self, action: Action<T>) -> Option<T> {
        let mut state = self
            .state
            .get(action.id.0 as usize)?
            .as_ref()?
            .write()
            .unwrap();
        let state = &mut *state as &mut dyn Any;
        state
            .downcast_mut::<ActionState<T>>()
            .expect("type mismatch")
            .queue
            .pop_front()
    }

    /// Observe the current state of `action`, if any
    pub fn get<T: 'static + Clone>(&self, action: Action<T>) -> Option<T> {
        let state = self
            .state
            .get(action.id.0 as usize)?
            .as_ref()?
            .read()
            .unwrap();
        let state = &*state as &dyn Any;
        Some(
            state
                .downcast_ref::<ActionState<T>>()
                .expect("type mismatch")
                .latest
                .clone(),
        )
    }

    /// Discard any state changes not consumed by calls to [`poll`](Self::poll)
    ///
    /// This must be called regularly (e.g. after running all input processing
    /// for a frame) to ensure that memory use does not grow without bound.
    pub fn flush(&mut self) {
        for state in self.state.iter().filter_map(Option::as_ref) {
            state.write().unwrap().flush();
        }
    }

    /// Update the state of `action` to `T`
    ///
    /// Most applications do not need to call this directly. It is usually
    /// called automatically by [`Bindings::handle`], which is in turn usually
    /// called by external event handlers.
    pub fn push<T: 'static + Clone>(
        &mut self,
        action: ActionId,
        value: T,
    ) -> Result<(), TypeError> {
        if self.state.len() <= action.0 as usize {
            self.state.resize_with(action.0 as usize + 1, || None);
        }
        match self.state[action.0 as usize] {
            ref mut slot @ None => {
                *slot = Some(Box::new(RwLock::new(ActionState {
                    queue: VecDeque::from_iter([value.clone()]),
                    latest: value,
                })));
            }
            Some(ref mut state) => {
                let mut state = state.write().unwrap();
                let Some(state) = (&mut *state as &mut dyn Any).downcast_mut::<ActionState<T>>()
                else {
                    return Err(TypeError {
                        expected: state.data_type_name(),
                        actual: type_name::<T>(),
                    });
                };
                state.latest.clone_from(&value);
                state.queue.push_back(value);
            }
        }
        Ok(())
    }
}

trait AnyState: Any {
    fn flush(&mut self);
    fn data_type_name(&self) -> &'static str;
}

struct ActionState<T> {
    queue: VecDeque<T>,
    latest: T,
}

impl<T: 'static> AnyState for ActionState<T> {
    fn flush(&mut self) {
        self.queue.clear();
    }

    fn data_type_name(&self) -> &'static str {
        type_name::<T>()
    }
}

/// A high-level semantic control used by an application
///
/// Actions should represent the control information your application cares
/// about. The inner type `T` is the type of data that can be supplied through
/// the action, i.e. its state.
///
/// Typical actions might include:
/// - "jump" and "shoot", of type `Action<()>`, representing instantaneous
///   events
/// - "forward"/"left"/"back"/"right" of type `Action<bool>` representing
///   whether a button is currently held
///
/// Actions should usually be hard-coded into an application, and constructed
/// early, before configuration parsing.
///
/// An [`Action`] is a lightweight handle that refers to state in the
/// [`Session`] used to create it. It must never be used with any other
/// [`Session`].
pub struct Action<T> {
    id: ActionId,
    _marker: PhantomData<T>,
}

impl<T> Action<T> {
    pub fn id(self) -> ActionId {
        self.id
    }
}

impl<T> Copy for Action<T> {}
impl<T> Clone for Action<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            _marker: PhantomData,
        }
    }
}

/// Untyped handle to an [`Action`] in some [`Session`]
// TODO: Nonzero
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ActionId(u32);
