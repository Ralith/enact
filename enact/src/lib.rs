use std::{
    any::{Any, TypeId},
    collections::VecDeque,
    fmt,
    hash::Hash,
    marker::PhantomData,
    sync::RwLock,
};

mod type_id_map;

use iddqd::BiHashMap;
use rustc_hash::FxHashMap;

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

    pub fn action_name(&self, id: ActionId) -> Option<&str> {
        Some(&self.actions.get1(&id)?.name)
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

pub trait Input: Hash + Eq + Clone + 'static {
    type Data: Clone;
}

#[derive(Default)]
pub struct Bindings {
    actions: TypeIdMap<Box<dyn AnyInputBindings>>,
}

impl Bindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind<I: Input>(&mut self, input: I, action: Action<I::Data>) {
        let bindings = self
            .actions
            .entry(TypeId::of::<I>())
            .or_insert_with(|| Box::new(InputBindings::<I>::default()));
        let bindings = (&mut **bindings as &mut dyn Any)
            .downcast_mut::<InputBindings<I>>()
            .unwrap();
        bindings.bindings.entry(input).or_default().push(action);
    }

    pub fn handle<I: Input>(&self, input: &I, data: I::Data, seat: &mut Seat) {
        let Some(actions) = self.actions.get(&TypeId::of::<I>()) else {
            return;
        };
        let Some(bindings) = (&**actions as &dyn Any)
            .downcast_ref::<InputBindings<I>>()
            .unwrap()
            .bindings
            .get(input)
        else {
            return;
        };
        for &binding in bindings {
            seat.push(binding, data.clone());
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
        }
    }
}

trait AnyInputBindings: Any {
    fn clone(&self) -> Box<dyn AnyInputBindings>;
}

impl<I: Input> AnyInputBindings for InputBindings<I> {
    fn clone(&self) -> Box<dyn AnyInputBindings> {
        Box::new(Clone::clone(self))
    }
}

struct InputBindings<I: Input> {
    bindings: FxHashMap<I, Vec<Action<I::Data>>>,
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

    pub fn push<T: 'static + Clone>(&mut self, action: Action<T>, value: T) {
        if self.state.len() <= action.id.0 as usize {
            self.state.resize_with(action.id.0 as usize + 1, || None);
        }
        match self.state[action.id.0 as usize] {
            ref mut slot @ None => {
                *slot = Some(Box::new(RwLock::new(ActionState {
                    queue: VecDeque::from_iter([value.clone()]),
                    latest: value,
                })));
            }
            Some(ref mut state) => {
                let mut state = state.write().unwrap();
                let state = &mut *state as &mut dyn Any;
                let state = state
                    .downcast_mut::<ActionState<T>>()
                    .expect("type mismatch");
                state.latest.clone_from(&value);
                state.queue.push_back(value);
            }
        }
    }
}

trait AnyState: Any {
    fn flush(&mut self);
}

struct ActionState<T> {
    queue: VecDeque<T>,
    latest: T,
}

impl<T: 'static> AnyState for ActionState<T> {
    fn flush(&mut self) {
        self.queue.clear();
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
