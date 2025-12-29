use std::{
    any::{Any, TypeId, type_name},
    collections::VecDeque,
    fmt,
    hash::Hash,
    marker::PhantomData,
    sync::RwLock,
};

mod type_id_map;

use iddqd::BiHashMap;
use rustc_hash::FxHashMap;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use type_id_map::TypeIdMap;

#[derive(Default)]
pub struct Session {
    actions: BiHashMap<ActionDefinition, rustc_hash::FxBuildHasher>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_action<T: 'static>(&mut self, name: &str) -> Action<T> {
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
            panic!("duplicate action: {name}");
        }
        Action {
            id,
            _marker: PhantomData,
        }
    }

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

    pub fn action_id(&self, name: &str) -> Option<ActionId> {
        Some(self.actions.get2(name)?.id)
    }

    pub fn action_name(&self, id: ActionId) -> &str {
        &self.actions.get1(&id).unwrap().name
    }

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

#[derive(Clone, Default)]
pub struct BindingsFactory {
    builders: FxHashMap<
        String,
        (
            TypeId,
            fn(&Session, &SourceConfig) -> (Box<dyn AnyInputBindings>, Vec<LoadError>),
        ),
    >,
}

impl BindingsFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<I: Input>(&mut self) {
        self.builders.insert(
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
                            errors.push(LoadError::TypeError {
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

    pub fn load(&self, session: &Session, config: &Config) -> (Bindings, Vec<LoadError>) {
        let mut bindings = Bindings::new();
        let mut errors = Vec::new();
        for source in &config.sources {
            let Some((ty, builder)) = self.builders.get(&source.ty) else {
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

#[derive(Debug, Clone)]
pub enum LoadError {
    UnknownSource {
        name: String,
    },
    UnknownAction {
        name: String,
    },
    UnknownInput {
        input: String,
    },
    TypeError {
        action_name: String,
        input: String,
        actual: &'static str,
        expected: Vec<&'static str>,
    },
}

#[derive(Default)]
pub struct Bindings {
    actions: TypeIdMap<Box<dyn AnyInputBindings>>,
}

impl Bindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save(&self, session: &Session) -> Config {
        Config {
            sources: self
                .actions
                .values()
                .map(|value| value.save(session))
                .collect(),
        }
    }

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
        }
        Ok(())
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

/// Serialized form of a seat's bindings
#[derive(Debug, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Config {
    pub sources: Vec<SourceConfig>,
}

/// Serialized form of the bindings for a seat from a specific input source
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SourceConfig {
    /// Identifies an input source in a configuration
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub ty: String,
    /// Maps action names to inputs from this source
    #[cfg_attr(feature = "serde", serde(with = "tuple_vec_map"))]
    pub bindings: Vec<(String, Vec<String>)>,
}

#[derive(Default)]
pub struct Seat {
    state: Vec<Option<Box<RwLock<dyn AnyState>>>>,
}

impl Seat {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn poll<T: 'static>(&self, action: &Action<T>) -> Option<T> {
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

    pub fn flush(&mut self) {
        for state in self.state.iter().filter_map(Option::as_ref) {
            state.write().unwrap().flush();
        }
    }

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

// TODO: Nonzero
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct ActionId(u32);
