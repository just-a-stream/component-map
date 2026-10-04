use crate::{ComponentMap, Keyed, WithArgs};

impl<Key, Args, Comp, FnInit> ComponentMap<Key, Args, Comp, FnInit> {
    pub fn init(entries: impl IntoIterator<Item = (Key, Args)>, init: FnInit) -> Self
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Key, &Args) -> Comp,
    {
        let map = entries
            .into_iter()
            .map(|(key, args)| {
                let component = (init)(&key, &args);
                (key, WithArgs { component, args })
            })
            .collect();

        Self { map, init }
    }

    pub fn reinit_all(&mut self) -> impl Iterator<Item = Keyed<&Key, Comp>>
    where
        FnInit: Fn(&Key, &Args) -> Comp,
    {
        let applied: Vec<_> = self
            .map
            .iter_mut()
            .map(|(key, component)| {
                let next = (self.init)(key, &component.args);
                let prev = std::mem::replace(&mut component.component, next);
                Keyed::new(key, prev)
            })
            .collect();

        applied.into_iter()
    }

    pub fn reinit(
        &mut self,
        keys: impl IntoIterator<Item = Key>,
    ) -> impl Iterator<Item = Keyed<Key, Option<Comp>>>
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Key, &Args) -> Comp,
    {
        let applied: Vec<_> = keys
            .into_iter()
            .map(|key| {
                let prev = self.map.get_mut(&key).map(|component| {
                    let next = (self.init)(&key, &component.args);
                    std::mem::replace(&mut component.component, next)
                });

                Keyed::new(key, prev)
            })
            .collect();

        applied.into_iter()
    }

    pub fn update_one(&mut self, key: Key, args: Args) -> Option<WithArgs<Args, Comp>>
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Key, &Args) -> Comp,
    {
        let component = (self.init)(&key, &args);

        self.map.insert(key, WithArgs { component, args })
    }

    pub fn update(
        &mut self,
        updates: impl IntoIterator<Item = (Key, Args)>,
    ) -> impl Iterator<Item = Keyed<Key, Option<WithArgs<Args, Comp>>>>
    where
        Key: Clone + Eq + std::hash::Hash,
        FnInit: Fn(&Key, &Args) -> Comp,
    {
        let applied: Vec<_> = updates
            .into_iter()
            .map(|(key, args)| {
                let prev = self.map.insert(
                    key.clone(),
                    WithArgs {
                        component: (self.init)(&key, &args),
                        args,
                    },
                );

                Keyed::new(key, prev)
            })
            .collect();

        applied.into_iter()
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
        let init = |_key: &&str, args: &Args| Counter(args.value);
        let manager = ComponentMap::init(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        );

        assert_eq!(manager.map.len(), 2);
        assert_eq!(manager.map.get("key1").unwrap().component, Counter(1));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(2));
        assert_eq!(manager.map.get("key1").unwrap().args.value, 1);
    }

    #[test]
    fn test_init_empty() {
        let init = |_key: &&str, args: &Args| Counter(args.value);
        let manager: ComponentMap<&str, Args, Counter, _> = ComponentMap::init([], init);

        assert_eq!(manager.map.len(), 0);
    }

    #[test]
    fn test_reinit_all() {
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let init = move |_key: &&str, args: &Args| {
            *call_count_clone.lock().unwrap() += 1;
            Counter(args.value * 2)
        };

        let mut manager = ComponentMap::init(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        );

        let prev_components: Vec<_> = manager.reinit_all().collect();

        assert_eq!(prev_components.len(), 2);

        let prev_values: Vec<_> = prev_components.iter().map(|k| &k.value.0).collect();
        assert!(prev_values.contains(&&2));
        assert!(prev_values.contains(&&4));

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(2));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(4));

        assert_eq!(*call_count.lock().unwrap(), 4);
    }

    #[test]
    fn test_reinit_all_empty() {
        let init = |_key: &&str, args: &Args| Counter(args.value);
        let mut manager: ComponentMap<&str, Args, Counter, _> = ComponentMap::init([], init);

        let results: Vec<_> = manager.reinit_all().collect();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_reinit_existing_key() {
        let init = |_key: &&str, args: &Args| Counter(args.value * 2);

        let mut manager = ComponentMap::init(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        );

        let results: Vec<_> = manager.reinit(["key1"]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key1");
        assert_eq!(results[0].value, Some(Counter(2)));

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(2));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(4));
    }

    #[test]
    fn test_reinit_nonexistent_key() {
        let init = |_key: &&str, args: &Args| Counter(args.value);

        let mut manager = ComponentMap::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager.reinit(["nonexistent"]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "nonexistent");
        assert_eq!(results[0].value, None);

        assert_eq!(manager.map.len(), 1);
    }

    #[test]
    fn test_update_existing_key() {
        let init = |_key: &&str, args: &Args| Counter(args.value);

        let mut manager = ComponentMap::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager.update([("key1", Args { value: 10 })]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key1");
        assert!(results[0].value.is_some());
        assert_eq!(results[0].value.as_ref().unwrap().component, Counter(1));
        assert_eq!(results[0].value.as_ref().unwrap().args.value, 1);

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(10));
        assert_eq!(manager.map.get("key1").unwrap().args.value, 10);
    }

    #[test]
    fn test_update_new_key() {
        let init = |_key: &&str, args: &Args| Counter(args.value);

        let mut manager = ComponentMap::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager.update([("key2", Args { value: 20 })]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key2");
        assert!(results[0].value.is_none());

        assert_eq!(manager.map.len(), 2);
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(20));
    }

    #[test]
    fn test_update_multiple_keys() {
        let init = |_key: &&str, args: &Args| Counter(args.value);

        let mut manager = ComponentMap::init([("key1", Args { value: 1 })], init);

        let results: Vec<_> = manager
            .update([
                ("key1", Args { value: 10 }),
                ("key2", Args { value: 20 }),
                ("key3", Args { value: 30 }),
            ])
            .collect();

        assert_eq!(results.len(), 3);
        assert_eq!(manager.map.len(), 3);
        assert_eq!(manager.map.get("key1").unwrap().component, Counter(10));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(20));
        assert_eq!(manager.map.get("key3").unwrap().component, Counter(30));
    }

    #[test]
    fn test_reinit_all_applies_without_consuming_results() {
        let calls = std::cell::Cell::new(0);
        let init = |_key: &&str, _args: &Args| {
            calls.set(calls.get() + 1);
            Counter(calls.get())
        };
        let mut manager = ComponentMap::init(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        );

        let _ = manager.reinit_all();

        assert!(manager.map.values().all(|entry| entry.component.0 > 2));
    }

    #[test]
    fn test_reinit_applies_without_consuming_results() {
        let calls = std::cell::Cell::new(0);
        let init = |_key: &&str, _args: &Args| {
            calls.set(calls.get() + 1);
            Counter(calls.get())
        };
        let mut manager = ComponentMap::init([("key1", Args { value: 1 })], init);

        let _ = manager.reinit(["key1"]);

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(2));
    }

    #[test]
    fn test_update_applies_without_consuming_results() {
        let init = |_key: &&str, args: &Args| Counter(args.value);
        let mut manager = ComponentMap::init([("key1", Args { value: 1 })], init);

        let _ = manager.update([("key1", Args { value: 10 }), ("key2", Args { value: 20 })]);

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(10));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(20));
    }
}
