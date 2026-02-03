use crate::{ComponentMap, Keyed, WithArgs};
use futures::future::join_all;

impl<Key, Args, Comp, FnInit> ComponentMap<Key, Args, Comp, FnInit> {
    pub async fn init_async(entries: impl IntoIterator<Item = (Key, Args)>, init: FnInit) -> Self
    where
        Key: Eq + std::hash::Hash,
        FnInit: AsyncFn(&Key, &Args) -> Comp + Clone,
    {
        let components_fut = entries.into_iter().map(|(key, args)| {
            let init = init.clone();
            async move {
                let component = (init)(&key, &args).await;
                (key, WithArgs { component, args })
            }
        });

        let map = join_all(components_fut).await.into_iter().collect();

        Self { map: map, init }
    }

    pub async fn reinit_all_async(&mut self) -> impl Iterator<Item = Keyed<&Key, Comp>>
    where
        FnInit: AsyncFn(&Key, &Args) -> Comp + Clone,
    {
        let next_components_fut = self
            .map
            .iter()
            .map(|(key, component)| (self.init)(key, &component.args));

        let next_components = join_all(next_components_fut).await;

        self.map
            .iter_mut()
            .zip(next_components)
            .map(|((key, prev), next)| {
                let prev = std::mem::replace(&mut prev.component, next);
                Keyed::new(key, prev)
            })
    }

    pub async fn reinit_async(
        &mut self,
        keys: impl IntoIterator<Item = Key>,
    ) -> impl Iterator<Item = Keyed<Key, Option<Comp>>>
    where
        Key: Eq + std::hash::Hash + Clone,
        FnInit: AsyncFn(&Key, &Args) -> Comp + Clone,
    {
        let next_components_fut = keys.into_iter().map(|key| {
            let init = self.init.clone();
            let args = self.map.get(&key).map(|component| &component.args);
            async move {
                let next = match args {
                    Some(args) => Some((init)(&key, args).await),
                    None => None,
                };
                Keyed::new(key, next)
            }
        });

        let results = join_all(next_components_fut).await;

        results.into_iter().map(|Keyed { key, value: next }| {
            let prev = next.and_then(|next| {
                self.map
                    .get_mut(&key)
                    .map(|component| std::mem::replace(&mut component.component, next))
            });
            Keyed::new(key, prev)
        })
    }

    pub async fn update_async(
        &mut self,
        updates: impl IntoIterator<Item = (Key, Args)>,
    ) -> impl Iterator<Item = Keyed<Key, Option<WithArgs<Args, Comp>>>>
    where
        Key: Clone + Eq + std::hash::Hash,
        FnInit: AsyncFn(&Key, &Args) -> Comp + Clone,
    {
        let updated_components_fut = updates.into_iter().map(|(key, args)| {
            let init = self.init.clone();
            async move {
                let component = (init)(&key, &args).await;
                (key, WithArgs { component, args })
            }
        });

        join_all(updated_components_fut)
            .await
            .into_iter()
            .map(|(key, component)| {
                let prev = self.map.insert(key.clone(), component);
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

    #[tokio::test]
    async fn test_init_async() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value) }
        };
        let manager = ComponentMap::init_async(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        )
        .await;

        assert_eq!(manager.map.len(), 2);
        assert_eq!(manager.map.get("key1").unwrap().component, Counter(1));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(2));
        assert_eq!(manager.map.get("key1").unwrap().args.value, 1);
    }

    #[tokio::test]
    async fn test_init_async_empty() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value) }
        };
        let manager: ComponentMap<&str, Args, Counter, _> =
            ComponentMap::init_async([], init).await;

        assert_eq!(manager.map.len(), 0);
    }

    #[tokio::test]
    async fn test_reinit_all_async() {
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let init = move |_key: &&str, args: &Args| {
            let call_count = call_count_clone.clone();
            let value = args.value;
            async move {
                *call_count.lock().unwrap() += 1;
                Counter(value * 2)
            }
        };

        let mut manager = ComponentMap::init_async(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        )
        .await;

        let prev_components: Vec<_> = manager.reinit_all_async().await.collect();

        assert_eq!(prev_components.len(), 2);
        assert_eq!(manager.map.get("key1").unwrap().component, Counter(2));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(4));

        // Should have called init 4 times (2 for init_async, 2 for reinit_all_async)
        assert_eq!(*call_count.lock().unwrap(), 4);
    }

    #[tokio::test]
    async fn test_reinit_all_async_empty() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value) }
        };
        let mut manager: ComponentMap<&str, Args, Counter, _> =
            ComponentMap::init_async([], init).await;

        let results: Vec<_> = manager.reinit_all_async().await.collect();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_reinit_async_existing_key() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value * 2) }
        };

        let mut manager = ComponentMap::init_async(
            [("key1", Args { value: 1 }), ("key2", Args { value: 2 })],
            init,
        )
        .await;

        let results: Vec<_> = manager.reinit_async(["key1"]).await.collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key1");
        assert_eq!(results[0].value, Some(Counter(2)));

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(2));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(4));
    }

    #[tokio::test]
    async fn test_reinit_async_multiple_keys() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value * 3) }
        };

        let mut manager = ComponentMap::init_async(
            [
                ("key1", Args { value: 1 }),
                ("key2", Args { value: 2 }),
                ("key3", Args { value: 3 }),
            ],
            init,
        )
        .await;

        let results: Vec<_> = manager.reinit_async(["key1", "key3"]).await.collect();

        assert_eq!(results.len(), 2);
        assert_eq!(manager.map.get("key1").unwrap().component, Counter(3));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(6));
        assert_eq!(manager.map.get("key3").unwrap().component, Counter(9));
    }

    #[tokio::test]
    async fn test_reinit_async_nonexistent_key() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value) }
        };

        let mut manager = ComponentMap::init_async([("key1", Args { value: 1 })], init).await;

        let results: Vec<_> = manager.reinit_async(["nonexistent"]).await.collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "nonexistent");
        assert_eq!(results[0].value, None);
        assert_eq!(manager.map.len(), 1);
    }

    #[tokio::test]
    async fn test_update_async_existing() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value) }
        };

        let mut manager = ComponentMap::init_async([("key1", Args { value: 1 })], init).await;

        let results: Vec<_> = manager
            .update_async([("key1", Args { value: 10 })])
            .await
            .collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key1");
        assert!(results[0].value.is_some());
        assert_eq!(results[0].value.as_ref().unwrap().component, Counter(1));

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(10));
        assert_eq!(manager.map.get("key1").unwrap().args.value, 10);
    }

    #[tokio::test]
    async fn test_update_async_new_key() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value) }
        };

        let mut manager = ComponentMap::init_async([("key1", Args { value: 1 })], init).await;

        let results: Vec<_> = manager
            .update_async([("key2", Args { value: 20 })])
            .await
            .collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "key2");
        assert!(results[0].value.is_none());

        assert_eq!(manager.map.len(), 2);
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(20));
    }

    #[tokio::test]
    async fn test_update_async_multiple() {
        let init = |_key: &&str, args: &Args| {
            let value = args.value;
            async move { Counter(value) }
        };

        let mut manager = ComponentMap::init_async([("key1", Args { value: 1 })], init).await;

        let results: Vec<_> = manager
            .update_async([
                ("key1", Args { value: 10 }),
                ("key2", Args { value: 20 }),
                ("key3", Args { value: 30 }),
            ])
            .await
            .collect();

        assert_eq!(results.len(), 3);
        assert_eq!(manager.map.len(), 3);
        assert_eq!(manager.map.get("key1").unwrap().component, Counter(10));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(20));
        assert_eq!(manager.map.get("key3").unwrap().component, Counter(30));
    }
}
