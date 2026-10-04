use crate::{ComponentMap, Keyed, WithArgs};

impl<Key, Args, Comp, FnInit> ComponentMap<Key, Args, Comp, FnInit> {
    pub fn try_init<Error>(
        entries: impl IntoIterator<Item = (Key, Args)>,
        init: FnInit,
    ) -> Result<Self, Error>
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Key, &Args) -> Result<Comp, Error>,
    {
        let map = entries
            .into_iter()
            .map(|(key, args)| {
                let component = (init)(&key, &args)?;
                Ok((key, WithArgs { component, args }))
            })
            .collect::<Result<_, _>>()?;

        Ok(Self { map, init })
    }

    pub fn try_reinit_all<Error>(
        &mut self,
    ) -> impl Iterator<Item = Keyed<&Key, Result<Comp, Error>>>
    where
        FnInit: Fn(&Key, &Args) -> Result<Comp, Error>,
    {
        let applied: Vec<_> = self
            .map
            .iter_mut()
            .map(|(key, component)| {
                let result = (self.init)(key, &component.args)
                    .map(|next| std::mem::replace(&mut component.component, next));

                Keyed::new(key, result)
            })
            .collect();

        applied.into_iter()
    }

    pub fn try_reinit<Error>(
        &mut self,
        keys: impl IntoIterator<Item = Key>,
    ) -> impl Iterator<Item = Keyed<Key, Result<Option<Comp>, Error>>>
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Key, &Args) -> Result<Comp, Error>,
    {
        let applied: Vec<_> = keys
            .into_iter()
            .map(|key| {
                let prev = self.map.get_mut(&key).map(|component| {
                    (self.init)(&key, &component.args)
                        .map(|next| std::mem::replace(&mut component.component, next))
                });

                Keyed::new(key, prev.transpose())
            })
            .collect();

        applied.into_iter()
    }

    #[allow(clippy::type_complexity)]
    pub fn try_update<Error>(
        &mut self,
        updates: impl IntoIterator<Item = (Key, Args)>,
    ) -> impl Iterator<Item = Keyed<Key, Result<Option<WithArgs<Args, Comp>>, Error>>>
    where
        Key: Clone + Eq + std::hash::Hash,
        FnInit: Fn(&Key, &Args) -> Result<Comp, Error>,
    {
        let applied: Vec<_> = updates
            .into_iter()
            .map(|(key, args)| {
                let result = (self.init)(&key, &args)
                    .map(|component| self.map.insert(key.clone(), WithArgs { component, args }));

                Keyed::new(key, result)
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
    struct FailArgs {
        value: usize,
        should_fail: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct TestError(String);

    #[test]
    fn test_try_init_success() {
        let init = |_key: &&str, args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let result = ComponentMap::try_init(
            [
                (
                    "key1",
                    FailArgs {
                        value: 1,
                        should_fail: false,
                    },
                ),
                (
                    "key2",
                    FailArgs {
                        value: 2,
                        should_fail: false,
                    },
                ),
            ],
            init,
        );

        assert!(result.is_ok());
        let manager = result.unwrap();
        assert_eq!(manager.map.len(), 2);
        assert_eq!(manager.map.get("key1").unwrap().component, Counter(1));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(2));
    }

    #[test]
    fn test_try_init_failure() {
        let init = |_key: &&str, args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let result = ComponentMap::try_init(
            [
                (
                    "key1",
                    FailArgs {
                        value: 1,
                        should_fail: false,
                    },
                ),
                (
                    "key2",
                    FailArgs {
                        value: 2,
                        should_fail: true,
                    },
                ),
            ],
            init,
        );

        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), TestError("Failed".to_string()));
    }

    #[test]
    fn test_try_init_empty() {
        let init = |_key: &&str, args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let result: Result<ComponentMap<&str, FailArgs, Counter, _>, TestError> =
            ComponentMap::try_init([], init);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().map.len(), 0);
    }

    #[test]
    fn test_try_reinit_all_success() {
        let init = |_key: &&str, args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value * 2))
            }
        };

        let mut manager = ComponentMap::try_init(
            [
                (
                    "key1",
                    FailArgs {
                        value: 1,
                        should_fail: false,
                    },
                ),
                (
                    "key2",
                    FailArgs {
                        value: 2,
                        should_fail: false,
                    },
                ),
            ],
            init,
        )
        .unwrap();

        let results: Vec<_> = manager.try_reinit_all().collect();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.value.is_ok()));

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(2));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(4));
    }

    #[test]
    fn test_try_reinit_all_with_failure() {
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let init = move |_key: &&str, args: &FailArgs| -> Result<Counter, TestError> {
            let count = *call_count_clone.lock().unwrap();
            *call_count_clone.lock().unwrap() += 1;

            if count >= 2 && args.should_fail {
                Err(TestError("Failed on reinit".to_string()))
            } else {
                Ok(Counter(args.value * 2))
            }
        };

        let mut manager = ComponentMap::try_init(
            [
                (
                    "key1",
                    FailArgs {
                        value: 1,
                        should_fail: false,
                    },
                ),
                (
                    "key2",
                    FailArgs {
                        value: 2,
                        should_fail: true,
                    },
                ),
            ],
            init,
        )
        .unwrap();

        let results: Vec<_> = manager.try_reinit_all().collect();

        assert_eq!(results.len(), 2);
        let failures: Vec<_> = results.iter().filter(|r| r.value.is_err()).collect();
        assert_eq!(failures.len(), 1);
        let successes: Vec<_> = results.iter().filter(|r| r.value.is_ok()).collect();
        assert_eq!(successes.len(), 1);
    }

    #[test]
    fn test_try_reinit_all_preserves_on_error() {
        let init = |_key: &&str, args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value * 2))
            }
        };

        let mut manager = ComponentMap::try_init(
            [(
                "key1",
                FailArgs {
                    value: 1,
                    should_fail: false,
                },
            )],
            init,
        )
        .unwrap();

        manager.map.get_mut("key1").unwrap().args.should_fail = true;

        let original_value = manager.map.get("key1").unwrap().component.clone();
        let _results: Vec<_> = manager.try_reinit_all().collect();

        assert_eq!(manager.map.get("key1").unwrap().component, original_value);
    }

    #[test]
    fn test_try_reinit_specific_keys_success() {
        let init = |_key: &&str, args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value * 3))
            }
        };

        let mut manager = ComponentMap::try_init(
            [
                (
                    "key1",
                    FailArgs {
                        value: 1,
                        should_fail: false,
                    },
                ),
                (
                    "key2",
                    FailArgs {
                        value: 2,
                        should_fail: false,
                    },
                ),
            ],
            init,
        )
        .unwrap();

        let results: Vec<_> = manager.try_reinit(["key1"]).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].value.as_ref().unwrap().is_some());
        assert_eq!(manager.map.get("key1").unwrap().component, Counter(3));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(6));
    }

    #[test]
    fn test_try_reinit_all_applies_without_consuming_results() {
        let calls = std::cell::Cell::new(0);
        let init = |_key: &&str, _args: &FailArgs| -> Result<Counter, TestError> {
            calls.set(calls.get() + 1);
            Ok(Counter(calls.get()))
        };
        let args = |value| FailArgs {
            value,
            should_fail: false,
        };
        let mut manager =
            ComponentMap::try_init([("key1", args(1)), ("key2", args(2))], init).unwrap();

        let _ = manager.try_reinit_all();

        assert!(manager.map.values().all(|entry| entry.component.0 > 2));
    }

    #[test]
    fn test_try_reinit_applies_without_consuming_results() {
        let calls = std::cell::Cell::new(0);
        let init = |_key: &&str, _args: &FailArgs| -> Result<Counter, TestError> {
            calls.set(calls.get() + 1);
            Ok(Counter(calls.get()))
        };
        let args = FailArgs {
            value: 1,
            should_fail: false,
        };
        let mut manager = ComponentMap::try_init([("key1", args)], init).unwrap();

        let _ = manager.try_reinit(["key1"]);

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(2));
    }

    #[test]
    fn test_try_update_applies_without_consuming_results() {
        let init = |_key: &&str, args: &FailArgs| -> Result<Counter, TestError> {
            Ok(Counter(args.value))
        };
        let args = |value| FailArgs {
            value,
            should_fail: false,
        };
        let mut manager = ComponentMap::try_init([("key1", args(1))], init).unwrap();

        let _ = manager.try_update([("key1", args(10)), ("key2", args(20))]);

        assert_eq!(manager.map.get("key1").unwrap().component, Counter(10));
        assert_eq!(manager.map.get("key2").unwrap().component, Counter(20));
    }
}
