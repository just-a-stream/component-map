use crate::{ComponentManager, Keyed, WithArgs};

impl<Key, Args, Comp, FnInit> ComponentManager<Key, Args, Comp, FnInit> {
    pub fn init(args: impl IntoIterator<Item = (Key, Args)>, init: FnInit) -> Self
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Args) -> Comp,
    {
        let map = args
            .into_iter()
            .map(|(key, args)| {
                (
                    key,
                    WithArgs {
                        component: (init)(&args),
                        args,
                    },
                )
            })
            .collect();

        Self { map, init }
    }

    pub fn reinit_all(&mut self) -> impl Iterator<Item = Keyed<&Key, Comp>>
    where
        FnInit: Fn(&Args) -> Comp,
    {
        self.map.iter_mut().map(|(key, component)| {
            let next = (self.init)(&component.args);
            let prev = std::mem::replace(&mut component.component, next);
            Keyed::new(key, prev)
        })
    }

    pub fn reinit(
        &mut self,
        keys: impl IntoIterator<Item = Key>,
    ) -> impl Iterator<Item = Keyed<Key, Option<Comp>>>
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Args) -> Comp,
    {
        keys.into_iter().map(|key| {
            let prev = self.map.get_mut(&key).map(|component| {
                let next = (self.init)(&component.args);
                std::mem::replace(&mut component.component, next)
            });

            Keyed::new(key, prev)
        })
    }

    pub fn update(
        &mut self,
        updates: impl IntoIterator<Item = (Key, Args)>,
    ) -> impl Iterator<Item = Keyed<Key, Option<WithArgs<Args, Comp>>>>
    where
        Key: Clone + Eq + std::hash::Hash,
        FnInit: Fn(&Args) -> Comp,
    {
        updates.into_iter().map(move |(key, args)| {
            let prev = self.map.insert(
                key.clone(),
                WithArgs {
                    component: (self.init)(&args),
                    args,
                },
            );

            Keyed::new(key, prev)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Counter(usize);

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Args {
        value: usize,
    }

    #[test]
    fn test_init() {
        let init = |args: &Args| Counter(args.value);
        let manager = ComponentManager::init(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        );

        assert_eq!(manager.components().len(), 2);
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(1)
        );
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(2)
        );
        assert_eq!(manager.components().get("key1").unwrap().args.value, 1);
    }

    #[test]
    fn test_init_empty() {
        let init = |args: &Args| Counter(args.value);
        let manager: ComponentManager<&str, Args, Counter, _> = ComponentManager::init([], init);

        assert_eq!(manager.components().len(), 0);
    }

    #[test]
    fn test_init_multiple_components() {
        let init = |args: &Args| Counter(args.value * 10);
        let manager = ComponentManager::init(
            [
                ("a", Args { value: 1 }),
                ("b", Args { value: 2 }),
                ("c", Args { value: 3 }),
                ("d", Args { value: 4 }),
            ],
            init,
        );

        assert_eq!(manager.components().len(), 4);
        assert_eq!(
            manager.components().get("a").unwrap().component,
            Counter(10)
        );
        assert_eq!(
            manager.components().get("d").unwrap().component,
            Counter(40)
        );
    }

    #[test]
    fn test_reinit_all() {
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let init = move |args: &Args| {
            *call_count_clone.lock().unwrap() += 1;
            Counter(args.value * 2)
        };

        let mut manager = ComponentManager::init(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        );

        // Collect to force evaluation
        let prev_components: Vec<_> = manager.reinit_all().collect();

        assert_eq!(prev_components.len(), 2);

        // Previous components should be the original values
        let prev_values: Vec<_> = prev_components.iter().map(|k| &k.value.0).collect();
        assert!(prev_values.contains(&&2));
        assert!(prev_values.contains(&&4));

        // Components should now have doubled values (checked after prev_components is used)
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(2)
        );
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(4)
        );

        // Should have called init 4 times (2 for init, 2 for reinit_all)
        assert_eq!(*call_count.lock().unwrap(), 4);
    }

    #[test]
    fn test_reinit_all_empty() {
        let init = |args: &Args| Counter(args.value);
        let mut manager: ComponentManager<&str, Args, Counter, _> =
            ComponentManager::init([], init);

        let results: Vec<_> = manager.reinit_all().collect();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_reinit_existing_key() {
        let init = |args: &Args| Counter(args.value * 2);

        let mut manager = ComponentManager::init(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        );

        let results: Vec<_> = manager.reinit(["key1"]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key1");
        assert_eq!(results[0].value, Some(Counter(2)));

        // key1 should be reinitialized (still 2 since args are still 1)
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(2)
        );
        // key2 should be unchanged
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(4)
        );
    }

    #[test]
    fn test_reinit_multiple_keys() {
        let init = |args: &Args| Counter(args.value * 3);

        let mut manager = ComponentManager::init(
            [
                ("key1", Args { value: 1 }),
                ("key2", Args { value: 2 }),
                ("key3", Args { value: 3 }),
            ],
            init,
        );

        let results: Vec<_> = manager.reinit(["key1", "key3"]).collect();

        assert_eq!(results.len(), 2);
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(3)
        );
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(6)
        );
        assert_eq!(
            manager.components().get("key3").unwrap().component,
            Counter(9)
        );
    }

    #[test]
    fn test_reinit_nonexistent_key() {
        let init = |args: &Args| Counter(args.value);

        let mut manager = ComponentManager::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager.reinit(["nonexistent"]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "nonexistent");
        assert_eq!(results[0].value, None);

        // Original component should be unchanged
        assert_eq!(manager.components().len(), 1);
    }

    #[test]
    fn test_reinit_mixed_existent_and_nonexistent() {
        let init = |args: &Args| Counter(args.value);

        let mut manager = ComponentManager::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager.reinit(["key1", "nonexistent"]).collect();

        assert_eq!(results.len(), 2);
        assert!(results[0].value.is_some() || results[1].value.is_some());
        assert!(results[0].value.is_none() || results[1].value.is_none());
    }

    #[test]
    fn test_update_existing_key() {
        let init = |args: &Args| Counter(args.value);

        let mut manager = ComponentManager::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager.update([("key1", Args { value: 10 })]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key1");
        assert!(results[0].value.is_some());
        assert_eq!(results[0].value.as_ref().unwrap().component, Counter(1));
        assert_eq!(results[0].value.as_ref().unwrap().args.value, 1);

        // Component should now be updated
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(10)
        );
        assert_eq!(manager.components().get("key1").unwrap().args.value, 10);
    }

    #[test]
    fn test_update_new_key() {
        let init = |args: &Args| Counter(args.value);

        let mut manager = ComponentManager::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager.update([("key2", Args { value: 20 })]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key2");
        assert!(results[0].value.is_none());

        // Should now have 2 components
        assert_eq!(manager.components().len(), 2);
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(20)
        );
    }

    #[test]
    fn test_update_multiple_keys() {
        let init = |args: &Args| Counter(args.value);

        let mut manager = ComponentManager::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager
            .update([
                ("key1", Args { value: 10 }),
                ("key2", Args { value: 20 }),
                ("key3", Args { value: 30 }),
            ])
            .collect();

        assert_eq!(results.len(), 3);
        assert_eq!(manager.components().len(), 3);
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(10)
        );
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(20)
        );
        assert_eq!(
            manager.components().get("key3").unwrap().component,
            Counter(30)
        );
    }

    #[test]
    fn test_components_accessors() {
        let init = |args: &Args| Counter(args.value);
        let mut manager = ComponentManager::init([("key1", Args { value: 1 })], init);

        // Test immutable access
        assert_eq!(manager.components().len(), 1);
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(1)
        );

        // Test mutable access
        manager.components_mut().get_mut("key1").unwrap().component = Counter(999);
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(999)
        );
    }

    #[test]
    fn test_fn_init_accessor() {
        let init = |args: &Args| Counter(args.value * 5);
        let manager = ComponentManager::init([("key1", Args { value: 1 })], init);

        let fn_init = manager.fn_init();
        let result = (fn_init)(&Args { value: 10 });
        assert_eq!(result, Counter(50));
    }
}
