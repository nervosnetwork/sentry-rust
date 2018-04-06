use std::mem;
use std::thread;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

use api::protocol::{Breadcrumb, Map, User, Value};
use client::Client;

lazy_static! {
    static ref PROCESS_STACK: Mutex<Stack> = Mutex::new(Stack::for_process());
}
thread_local! {
    static THREAD_STACK: RefCell<Stack> = RefCell::new(Stack::for_thread());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StackType {
    Process,
    Thread,
}

#[derive(Debug)]
pub struct Stack {
    layers: Vec<StackLayer>,
    ty: StackType,
}

/// Holds contextual data for the current scope.
///
/// This is optional on a lot of calls to the client itself.
#[derive(Default, Debug)]
pub struct Scope {
    pub breadcrumbs: VecDeque<Breadcrumb>,
    pub user: Option<User>,
    pub extra: Option<Map<String, Value>>,
    pub tags: Option<Map<String, String>>,
}

#[derive(Default, Debug)]
struct StackLayer {
    client: Option<Arc<Client>>,
    scope: Scope,
}

impl Stack {
    pub fn for_process() -> Stack {
        Stack {
            layers: vec![Default::default()],
            ty: StackType::Process,
        }
    }

    pub fn for_thread() -> Stack {
        Stack {
            layers: vec![
                StackLayer {
                    client: with_stack(|stack| stack.client()),
                    scope: Default::default(),
                },
            ],
            ty: StackType::Thread,
        }
    }

    pub fn push(&mut self) {
        self.layers.push(Default::default());
    }

    pub fn pop(&mut self) {
        if self.layers.len() <= 1 {
            panic!("Pop from empty {:?} stack", self.ty)
        }
        self.layers.pop().unwrap();
    }

    pub fn bind_client(&mut self, client: Arc<Client>) {
        let depth = self.layers.len() - 1;
        self.layers[depth].client = Some(client);
    }

    pub fn client(&self) -> Option<Arc<Client>> {
        self.layers[self.layers.len() - 1].client.clone()
    }

    pub fn scope_mut(&mut self) -> &mut Scope {
        let idx = self.layers.len() - 1;
        &mut self.layers[idx].scope
    }
}

fn is_main_thread() -> bool {
    let thread = thread::current();
    let raw_id: u64 = unsafe { mem::transmute(thread.id()) };
    raw_id == 0
}

fn with_process_stack<F, R>(f: F) -> R
where
    F: FnOnce(&mut Stack) -> R,
{
    f(&mut PROCESS_STACK.lock().unwrap())
}

fn with_thread_stack<F, R>(f: F) -> R
where
    F: FnOnce(&mut Stack) -> R,
{
    THREAD_STACK.with(|stack| f(&mut *stack.borrow_mut()))
}

pub fn with_stack<F, R>(f: F) -> R
where
    F: FnOnce(&mut Stack) -> R,
{
    if is_main_thread() {
        with_process_stack(f)
    } else {
        with_thread_stack(f)
    }
}

/// Crate internal helper for working with clients and scopes.
pub fn with_client_and_scope<F, R>(f: F) -> R
where
    F: FnOnce(Arc<Client>, &mut Scope) -> R,
    R: Default,
{
    with_stack(|stack| {
        if let Some(client) = stack.client() {
            f(client, stack.scope_mut())
        } else {
            Default::default()
        }
    })
}

/// Pushes a new scope on the stack.
///
/// If client is `None` then the currently bound client is propagated to the
/// new scope.
pub fn push_scope(client: Option<Arc<Client>>) {
    with_stack(|stack| {
        stack.push();
        if let Some(client) = client {
            stack.bind_client(client);
        }
    })
}

/// Pops the inner scope.
pub fn pop_scope() {
    with_stack(|stack| {
        stack.pop();
    });
}