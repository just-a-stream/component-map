use crate::{ComponentManager, Keyed, WithArgs};

impl<Key, Args, Comp, FnInit> ComponentManager<Key, Args, Comp, FnInit> {
    pub fn try_init<Error>(
        args: impl IntoIterator<Item = (Key, Args)>,
        init: FnInit,
    ) -> Result<Self, Error>
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Args) -> Result<Comp, Error>,
    {
        let map = args
            .into_iter()
            .map(|(key, args)| {
                let component = (init)(&args)?;
                Ok((key, WithArgs { component, args }))
            })
            .collect::<Result<_, _>>()?;

        Ok(Self { map, init })
    }

    pub fn try_reinit_all<Error>(
        &mut self,
    ) -> impl Iterator<Item = Keyed<&Key, Result<Comp, Error>>>
    where
        FnInit: Fn(&Args) -> Result<Comp, Error>,
    {
        self.map.iter_mut().map(|(key, component)| {
            let result = (self.init)(&component.args)
                .map(|next| std::mem::replace(&mut component.component, next));

            Keyed::new(key, result)
        })
    }

    pub fn try_reinit<Error>(
        &mut self,
        keys: impl IntoIterator<Item = Key>,
    ) -> impl Iterator<Item = Keyed<Key, Option<Result<Comp, Error>>>>
    where
        Key: Eq + std::hash::Hash,
        FnInit: Fn(&Args) -> Result<Comp, Error>,
    {
        keys.into_iter().map(|key| {
            let prev = self.map.get_mut(&key).map(|component| {
                (self.init)(&component.args)
                    .map(|next| std::mem::replace(&mut component.component, next))
            });

            Keyed::new(key, prev)
        })
    }

    pub fn try_update<Error>(
        &mut self,
        updates: impl IntoIterator<Item = (Key, Args)>,
    ) -> impl Iterator<Item = Keyed<Key, Option<Result<WithArgs<Args, Comp>, Error>>>>
    where
        Key: Clone + Eq + std::hash::Hash,
        FnInit: Fn(&Args) -> Result<Comp, Error>,
    {
        updates.into_iter().map(move |(key, args)| {
            let result = (self.init)(&args)
                .map(|component| self.map.insert(key.clone(), WithArgs { component, args }));

            Keyed::new(key, result.transpose())
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
    struct FailArgs {
        value: usize,
        should_fail: bool,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct TestError(String);

    #[test]
    fn test_try_init_success() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let result = ComponentManager::try_init(
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
        assert_eq!(manager.components().len(), 2);
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(1)
        );
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(2)
        );
    }

    #[test]
    fn test_try_init_failure() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let result = ComponentManager::try_init(
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
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let result: Result<ComponentManager<&str, FailArgs, Counter, _>, TestError> =
            ComponentManager::try_init([], init);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().components().len(), 0);
    }

    #[test]
    fn test_try_init_all_fail() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let result = ComponentManager::try_init(
            [
                (
                    "key1",
                    FailArgs {
                        value: 1,
                        should_fail: true,
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
    }

    #[test]
    fn test_try_reinit_all_success() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value * 2))
            }
        };

        let mut manager = ComponentManager::try_init(
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

        // Check that components are updated
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(2)
        );
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(4)
        );
    }

    #[test]
    fn test_try_reinit_all_with_failure() {
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let init = move |args: &FailArgs| -> Result<Counter, TestError> {
            let count = *call_count_clone.lock().unwrap();
            *call_count_clone.lock().unwrap() += 1;

            // Fail on reinit (after initial successful init)
            if count >= 2 && args.should_fail {
                Err(TestError("Failed on reinit".to_string()))
            } else {
                Ok(Counter(args.value * 2))
            }
        };

        let mut manager = ComponentManager::try_init(
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
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value * 2))
            }
        };

        let mut manager = ComponentManager::try_init(
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

        // Change args to make it fail
        manager
            .components_mut()
            .get_mut("key1")
            .unwrap()
            .args
            .should_fail = true;

        let original_value = manager.components().get("key1").unwrap().component.clone();
        let _results: Vec<_> = manager.try_reinit_all().collect();

        // Component should remain unchanged on error
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            original_value
        );
    }

    #[test]
    fn test_try_reinit_specific_keys_success() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value * 3))
            }
        };

        let mut manager = ComponentManager::try_init(
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
        assert!(results[0].value.as_ref().unwrap().is_ok());
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(3)
        );
        // key2 should be unchanged from initial
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(6)
        );
    }

    #[test]
    fn test_try_reinit_nonexistent_key() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let mut manager = ComponentManager::try_init(
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

        let results: Vec<_> = manager.try_reinit(["nonexistent"]).collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "nonexistent");
        assert!(results[0].value.is_none());
    }

    #[test]
    fn test_try_reinit_with_failure() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let mut manager = ComponentManager::try_init(
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

        // Set to fail
        manager
            .components_mut()
            .get_mut("key1")
            .unwrap()
            .args
            .should_fail = true;

        let results: Vec<_> = manager.try_reinit(["key1"]).collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].value.as_ref().unwrap().is_err());
    }

    #[test]
    fn test_try_update_new_key_success() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let mut manager = ComponentManager::try_init(
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

        let results: Vec<_> = manager
            .try_update([(
                "key2",
                FailArgs {
                    value: 20,
                    should_fail: false,
                },
            )])
            .collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].value.is_none());
        assert_eq!(manager.components().len(), 2);
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(20)
        );
    }

    #[test]
    fn test_try_update_existing_key_success() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let mut manager = ComponentManager::try_init(
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

        let results: Vec<_> = manager
            .try_update([(
                "key1",
                FailArgs {
                    value: 10,
                    should_fail: false,
                },
            )])
            .collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].value.is_some());
        let prev = results[0].value.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(prev.component, Counter(1));

        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(10)
        );
    }

    #[test]
    fn test_try_update_failure() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let mut manager = ComponentManager::try_init(
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

        let results: Vec<_> = manager
            .try_update([(
                "key2",
                FailArgs {
                    value: 20,
                    should_fail: true,
                },
            )])
            .collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].value.is_some());
        assert!(results[0].value.as_ref().unwrap().is_err());

        // Should not insert on error
        assert_eq!(manager.components().len(), 1);
        assert!(manager.components().get("key2").is_none());
    }

    #[test]
    fn test_try_update_multiple_mixed() {
        let init = |args: &FailArgs| -> Result<Counter, TestError> {
            if args.should_fail {
                Err(TestError("Failed".to_string()))
            } else {
                Ok(Counter(args.value))
            }
        };

        let mut manager = ComponentManager::try_init(
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

        let results: Vec<_> = manager
            .try_update([
                (
                    "key2",
                    FailArgs {
                        value: 20,
                        should_fail: false,
                    },
                ),
                (
                    "key3",
                    FailArgs {
                        value: 30,
                        should_fail: true,
                    },
                ),
                (
                    "key4",
                    FailArgs {
                        value: 40,
                        should_fail: false,
                    },
                ),
            ])
            .collect();

        assert_eq!(results.len(), 3);

        // Check that only successful updates were inserted
        assert_eq!(manager.components().len(), 3); // key1, key2, key4
        assert!(manager.components().get("key2").is_some());
        assert!(manager.components().get("key3").is_none());
        assert!(manager.components().get("key4").is_some());
    }
}
