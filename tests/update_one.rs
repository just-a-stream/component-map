use component_map::ComponentMap;
use futures::{FutureExt, executor::block_on, future};
use std::{cell::Cell, collections::HashMap, rc::Rc, task::Poll};

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct Key(u32);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
enum InitError {
    Rejected,
}

#[derive(Debug)]
struct Context {
    calls: Cell<u32>,
}

impl Context {
    async fn init(&self, key: &Key, args: &u32) -> Rc<u32> {
        suspend_once().await;
        self.calls.set(self.calls.get() + 1);
        Rc::new(key.0 + args + self.calls.get())
    }
}

#[derive(Debug)]
struct DropGuard(Rc<Cell<u32>>);

impl Drop for DropGuard {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn sync_inserts_and_replaces_without_cloning_keys() {
    let mut components = ComponentMap::new(HashMap::new(), |key: &Key, args: &u32| key.0 + args);

    assert!(components.update_one(Key(10), 2).is_none());
    let previous = components.update_one(Key(10), 4).unwrap();

    assert_eq!(previous.args, 2);
    assert_eq!(previous.component, 12);
    assert_eq!(components.map[&Key(10)].args, 4);
    assert_eq!(components.map[&Key(10)].component, 14);
}

#[test]
fn sync_failure_preserves_applied_args_for_reinitialisation() {
    let mut components = ComponentMap::new(HashMap::new(), |key: &Key, args: &u32| {
        if *args == 0 {
            return Err(InitError::Rejected);
        }
        Ok(key.0 + args)
    });

    assert!(components.try_update_one(Key(10), 2).unwrap().is_none());
    let previous = components.try_update_one(Key(10), 4).unwrap().unwrap();
    assert_eq!(previous.args, 2);
    assert_eq!(previous.component, 12);

    assert!(matches!(
        components.try_update_one(Key(10), 0),
        Err(InitError::Rejected)
    ));
    assert!(matches!(
        components.try_update_one(Key(20), 0),
        Err(InitError::Rejected)
    ));

    assert!(!components.map.contains_key(&Key(20)));
    assert_eq!(components.map[&Key(10)].args, 4);
    assert_eq!(components.map[&Key(10)].component, 14);
    assert_eq!(components.try_reinit_all().next().unwrap().value, Ok(14));
    assert_eq!(components.map[&Key(10)].component, 14);
}

#[test]
fn async_inserts_and_replaces_without_cloning_keys() {
    block_on(async {
        let mut components = ComponentMap::new(HashMap::new(), async |key: &Key, args: &u32| {
            suspend_once().await;
            key.0 + args
        });

        assert!(components.update_one_async(Key(10), 2).await.is_none());
        let previous = components.update_one_async(Key(10), 4).await.unwrap();

        assert_eq!(previous.args, 2);
        assert_eq!(previous.component, 12);
        assert_eq!(components.map[&Key(10)].args, 4);
        assert_eq!(components.map[&Key(10)].component, 14);
    });
}

#[test]
fn async_failure_preserves_applied_args_for_reinitialisation() {
    block_on(async {
        let mut components = ComponentMap::new(HashMap::new(), async |key: &Key, args: &u32| {
            suspend_once().await;
            if *args == 0 {
                return Err(InitError::Rejected);
            }
            Ok(key.0 + args)
        });

        assert!(
            components
                .try_update_one_async(Key(10), 2)
                .await
                .unwrap()
                .is_none()
        );
        let previous = components
            .try_update_one_async(Key(10), 4)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(previous.args, 2);
        assert_eq!(previous.component, 12);

        assert!(matches!(
            components.try_update_one_async(Key(10), 0).await,
            Err(InitError::Rejected)
        ));
        assert!(matches!(
            components.try_update_one_async(Key(20), 0).await,
            Err(InitError::Rejected)
        ));

        assert!(!components.map.contains_key(&Key(20)));
        assert_eq!(components.map[&Key(10)].args, 4);
        assert_eq!(components.map[&Key(10)].component, 14);
        assert_eq!(
            components
                .try_reinit_all_async()
                .await
                .next()
                .unwrap()
                .value,
            Ok(14)
        );
        assert_eq!(components.map[&Key(10)].component, 14);
    });
}

#[test]
fn async_factory_can_lend_non_clone_local_context_across_suspension() {
    block_on(async {
        let context = Context {
            calls: Cell::new(0),
        };
        let init = async move |key: &Key, args: &u32| context.init(key, args).await;
        let mut components = ComponentMap::init_async([(Key(10), 2)], init).await;

        let previous = components.update_one_async(Key(10), 4).await.unwrap();

        assert_eq!(*previous.component, 13);
        assert_eq!(*components.map[&Key(10)].component, 16);
    });
}

#[test]
fn cancelled_preparation_drops_its_guard_and_preserves_the_old_entry() {
    let dropped = Rc::new(Cell::new(0));
    let init = async |key: &Key, args: &u32| {
        if *args == 0 {
            let _guard = DropGuard(Rc::clone(&dropped));
            future::pending::<()>().await;
        }
        Ok::<_, InitError>(key.0 + args)
    };
    let mut components = block_on(ComponentMap::try_init_async([(Key(10), 2)], init)).unwrap();

    assert!(
        components
            .try_update_one_async(Key(10), 0)
            .now_or_never()
            .is_none()
    );

    assert_eq!(dropped.get(), 1);
    assert_eq!(components.map[&Key(10)].args, 2);
    assert_eq!(components.map[&Key(10)].component, 12);
    assert!(
        block_on(components.try_update_one_async(Key(10), 4))
            .unwrap()
            .is_some()
    );
    assert_eq!(components.map[&Key(10)].component, 14);
}

#[test]
fn cancelled_infallible_preparation_preserves_the_old_entry() {
    let init = async |key: &Key, args: &u32| {
        if *args == 0 {
            future::pending::<()>().await;
        }
        key.0 + args
    };
    let mut components = block_on(ComponentMap::init_async([(Key(10), 2)], init));

    assert!(
        components
            .update_one_async(Key(10), 0)
            .now_or_never()
            .is_none()
    );

    assert_eq!(components.map[&Key(10)].args, 2);
    assert_eq!(components.map[&Key(10)].component, 12);
}

async fn suspend_once() {
    let mut suspended = false;
    future::poll_fn(|context| {
        if suspended {
            Poll::Ready(())
        } else {
            suspended = true;
            context.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}
