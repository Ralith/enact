use std::{any::Any, collections::VecDeque, marker::PhantomData, sync::RwLock};

use slab::Slab;

#[derive(Default)]
pub struct Session {
    actions: Slab<()>,
}

impl Session {
    pub fn create_action<T: 'static>(&mut self) -> Action<T> {
        Action {
            id: ActionId(self.actions.insert(())),
            _marker: PhantomData,
        }
    }
}

#[derive(Default)]
pub struct Seat {
    state: Vec<Option<Box<RwLock<dyn AnyState>>>>,
}

impl Seat {
    pub fn poll<T: 'static>(&self, action: &Action<T>) -> Option<T> {
        let mut state = self.state.get(action.id.0)?.as_ref()?.write().unwrap();
        let state = &mut *state as &mut dyn Any;
        state
            .downcast_mut::<ActionState<T>>()
            .expect("type mismatch")
            .queue
            .pop_front()
    }

    pub fn get<T: 'static + Clone>(&self, action: &Action<T>) -> Option<T> {
        let state = self.state.get(action.id.0)?.as_ref()?.read().unwrap();
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

    pub fn push<T: 'static + Clone>(&mut self, action: &Action<T>, value: T) {
        if self.state.len() < action.id.0 {
            self.state.resize_with(action.id.0 + 1, || None);
        }
        match self.state[action.id.0] {
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

pub struct Action<T: 'static> {
    id: ActionId,
    _marker: PhantomData<T>,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
struct ActionId(usize);
