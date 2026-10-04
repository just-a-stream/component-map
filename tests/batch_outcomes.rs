use component_map::ComponentMap;
use futures::{FutureExt, executor::block_on, future};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    task::Poll,
};

#[derive(Debug)]
struct Tracked {
    generation: u32,
    dropped: Rc<RefCell<Vec<u32>>>,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.generation);
    }
}

#[test]
fn sync_reinit_distinguishes_missing_from_failed_and_returns_old_values() {
    let rejected = Cell::new(false);
    let revision = Cell::new(1);
    let init = |key: &&str, args: &u32| {
        if *key == "rejected" && rejected.get() {
            return Err("rejected");
        }
        Ok(args + revision.get())
    };
    let mut components = ComponentMap::try_init([("valid", 10), ("rejected", 20)], init).unwrap();
    rejected.set(true);
    revision.set(2);

    let results: Vec<_> = components
        .try_reinit(["valid", "rejected", "missing"])
        .collect();

    assert_eq!(results[0].key, "valid");
    assert_eq!(results[0].value, Ok(Some(11)));
    assert_eq!(results[1].key, "rejected");
    assert_eq!(results[1].value, Err("rejected"));
    assert_eq!(results[2].key, "missing");
    assert_eq!(results[2].value, Ok(None));
    assert_eq!(components.map["valid"].component, 12);
    assert_eq!(components.map["rejected"].component, 21);
    assert_eq!(components.map["rejected"].args, 20);
}

#[test]
fn async_reinit_distinguishes_missing_from_failed_and_returns_old_values() {
    block_on(async {
        let rejected = Cell::new(false);
        let revision = Cell::new(1);
        let init = async |key: &&str, args: &u32| {
            if *key == "rejected" && rejected.get() {
                return Err("rejected");
            }
            Ok(args + revision.get())
        };
        let mut components = ComponentMap::try_init_async([("valid", 10), ("rejected", 20)], init)
            .await
            .unwrap();
        rejected.set(true);
        revision.set(2);

        let results: Vec<_> = components
            .try_reinit_async(["valid", "rejected", "missing"])
            .await
            .collect();

        assert_eq!(results[0].key, "valid");
        assert_eq!(results[0].value, Ok(Some(11)));
        assert_eq!(results[1].key, "rejected");
        assert_eq!(results[1].value, Err("rejected"));
        assert_eq!(results[2].key, "missing");
        assert_eq!(results[2].value, Ok(None));
        assert_eq!(components.map["valid"].component, 12);
        assert_eq!(components.map["rejected"].component, 21);
        assert_eq!(components.map["rejected"].args, 20);
    });
}

#[test]
fn sync_update_reports_each_duplicate_in_input_order_including_failures() {
    let init = |_key: &&str, args: &u32| {
        if *args == 0 {
            Err("rejected")
        } else {
            Ok(args * 10)
        }
    };
    let mut components = ComponentMap::try_init([("existing", 1)], init).unwrap();

    let results: Vec<_> = components
        .try_update([
            ("existing", 2),
            ("existing", 0),
            ("existing", 3),
            ("rejected", 0),
            ("new", 4),
        ])
        .map(|result| {
            (
                result.key,
                result
                    .value
                    .map(|previous| previous.map(|entry| (entry.args, entry.component))),
            )
        })
        .collect();

    assert_eq!(
        results,
        [
            ("existing", Ok(Some((1, 10)))),
            ("existing", Err("rejected")),
            ("existing", Ok(Some((2, 20)))),
            ("rejected", Err("rejected")),
            ("new", Ok(None)),
        ]
    );
    assert_eq!(components.map["existing"].args, 3);
    assert_eq!(components.map["existing"].component, 30);
    assert_eq!(components.map["new"].args, 4);
    assert_eq!(components.map["new"].component, 40);
    assert!(!components.map.contains_key("rejected"));
}

#[test]
fn async_update_installs_duplicates_in_input_order_despite_completion_order() {
    block_on(async {
        let completed = RefCell::new(Vec::new());
        let init = async |_key: &&str, args: &u32| {
            if *args == 2 {
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
            completed.borrow_mut().push(*args);
            if *args == 0 {
                Err("rejected")
            } else {
                Ok(args * 10)
            }
        };
        let mut components = ComponentMap::try_init_async([("existing", 1)], init)
            .await
            .unwrap();

        let results: Vec<_> = components
            .try_update_async([
                ("existing", 2),
                ("existing", 0),
                ("existing", 3),
                ("rejected", 0),
                ("new", 4),
            ])
            .await
            .map(|result| {
                (
                    result.key,
                    result
                        .value
                        .map(|previous| previous.map(|entry| (entry.args, entry.component))),
                )
            })
            .collect();

        assert_eq!(completed.borrow().as_slice(), [1, 0, 3, 0, 4, 2]);
        assert_eq!(
            results,
            [
                ("existing", Ok(Some((1, 10)))),
                ("existing", Err("rejected")),
                ("existing", Ok(Some((2, 20)))),
                ("rejected", Err("rejected")),
                ("new", Ok(None)),
            ]
        );
        assert_eq!(components.map["existing"].args, 3);
        assert_eq!(components.map["existing"].component, 30);
        assert_eq!(components.map["new"].args, 4);
        assert_eq!(components.map["new"].component, 40);
        assert!(!components.map.contains_key("rejected"));
    });
}

#[test]
fn cancelling_a_preparing_batch_drops_candidates_without_applying_them() {
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let init = async |_key: &&str, args: &u32| {
        if *args == 3 {
            future::pending::<()>().await;
        }
        Ok::<_, ()>(Tracked {
            generation: *args,
            dropped: Rc::clone(&dropped),
        })
    };
    let mut components = block_on(ComponentMap::try_init_async([("old", 1)], init)).unwrap();

    let outcome = components
        .try_update_async([("old", 2), ("new", 3)])
        .now_or_never();
    assert!(outcome.is_none());
    drop(outcome);

    assert_eq!(dropped.borrow().as_slice(), [2]);
    assert_eq!(components.map.len(), 1);
    assert_eq!(components.map["old"].args, 1);
    assert_eq!(components.map["old"].component.generation, 1);
}
