use crate::{ComponentMap, Keyed, WithArgs};
use futures::future::join_all;

impl<Key, Args, Comp, FnInit> ComponentMap<Key, Args, Comp, FnInit> {
    pub async fn try_init_async<Error>(
        args: impl IntoIterator<Item = (Key, Args)>,
        init: FnInit,
    ) -> Result<Self, Error>
    where
        Key: Eq + std::hash::Hash,
        FnInit: AsyncFn(&Args) -> Result<Comp, Error> + Clone,
    {
        let components_fut = args.into_iter().map(|(key, args)| {
            let init = init.clone();
            async move {
                let result = (init)(&args)
                    .await
                    .map(|component| WithArgs { component, args });

                (key, result)
            }
        });

        let map = join_all(components_fut)
            .await
            .into_iter()
            .map(|(key, result)| result.map(|component| (key, component)))
            .collect::<Result<_, _>>()?;

        Ok(Self { map, init })
    }

    pub async fn try_reinit_all_async<Error>(
        &mut self,
    ) -> impl Iterator<Item = Keyed<&Key, Result<Comp, Error>>>
    where
        Key: Clone,
        FnInit: AsyncFn(&Args) -> Result<Comp, Error> + Clone,
    {
        let next_components_fut = self
            .map
            .values()
            .map(|component| (self.init)(&component.args));

        let next_components = join_all(next_components_fut).await;

        self.map
            .iter_mut()
            .zip(next_components)
            .map(|((key, prev), result)| {
                let result = result.map(|next| std::mem::replace(&mut prev.component, next));

                Keyed::new(key, result)
            })
    }

    pub async fn try_reinit_async<Error>(
        &mut self,
        keys: impl IntoIterator<Item = Key>,
    ) -> impl Iterator<Item = Keyed<Key, Option<Result<Comp, Error>>>>
    where
        Key: Eq + std::hash::Hash + Clone,
        FnInit: AsyncFn(&Args) -> Result<Comp, Error> + Clone,
    {
        let next_components_fut = keys.into_iter().map(|key| {
            let init = self.init.clone();
            let args = self.map.get(&key).map(|component| &component.args);
            async move {
                let result = match args {
                    Some(args) => Some((init)(args).await),
                    None => None,
                };
                Keyed::new(key, result)
            }
        });

        let results = join_all(next_components_fut).await;

        results.into_iter().map(|Keyed { key, value: result }| {
            let prev = result
                .map(|result| {
                    result.map(|next| {
                        self.map
                            .get_mut(&key)
                            .map(|component| std::mem::replace(&mut component.component, next))
                    })
                })
                .transpose()
                .map(Option::flatten);

            Keyed::new(key, prev.transpose())
        })
    }

    pub async fn try_update_async<Error>(
        &mut self,
        updates: impl IntoIterator<Item = (Key, Args)>,
    ) -> impl Iterator<Item = Keyed<Key, Option<Result<WithArgs<Args, Comp>, Error>>>>
    where
        Key: Clone + Eq + std::hash::Hash,
        FnInit: AsyncFn(&Args) -> Result<Comp, Error> + Clone,
    {
        let updated_components_fut = updates.into_iter().map(|(key, args)| {
            let init = self.init.clone();
            async move {
                let result = (init)(&args)
                    .await
                    .map(|component| WithArgs { component, args });

                (key, result)
            }
        });

        join_all(updated_components_fut)
            .await
            .into_iter()
            .map(|(key, result)| {
                let result = result.map(|component| self.map.insert(key.clone(), component));

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

    #[tokio::test]
    async fn test_try_init_async_success() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value))
                }
            }
        };

        let result = ComponentMap::try_init_async(
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
        .await;

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

    #[tokio::test]
    async fn test_try_init_async_failure() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value))
                }
            }
        };

        let result = ComponentMap::try_init_async(
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
        .await;

        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), TestError("Failed".to_string()));
    }

    #[tokio::test]
    async fn test_try_init_async_empty() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value))
                }
            }
        };

        let result: Result<ComponentMap<&str, FailArgs, Counter, _>, TestError> =
            ComponentMap::try_init_async([], init).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().components().len(), 0);
    }

    #[tokio::test]
    async fn test_try_reinit_all_async_success() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value * 2))
                }
            }
        };

        let mut manager = ComponentMap::try_init_async(
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
        .await
        .unwrap();

        let results: Vec<_> = manager.try_reinit_all_async().await.collect();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.value.is_ok()));

        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(2)
        );
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(4)
        );
    }

    #[tokio::test]
    async fn test_try_reinit_all_async_with_failure() {
        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = call_count.clone();

        let init = move |args: &FailArgs| {
            let call_count = call_count_clone.clone();
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                let count = *call_count.lock().unwrap();
                *call_count.lock().unwrap() += 1;

                if count >= 2 && should_fail {
                    Err(TestError("Failed on reinit".to_string()))
                } else {
                    Ok(Counter(value * 2))
                }
            }
        };

        let mut manager = ComponentMap::try_init_async(
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
        .await
        .unwrap();

        let results: Vec<_> = manager.try_reinit_all_async().await.collect();

        assert_eq!(results.len(), 2);
        let failures: Vec<_> = results.iter().filter(|r| r.value.is_err()).collect();
        assert_eq!(failures.len(), 1);
        let successes: Vec<_> = results.iter().filter(|r| r.value.is_ok()).collect();
        assert_eq!(successes.len(), 1);
    }

    #[tokio::test]
    async fn test_try_reinit_all_async_empty() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value))
                }
            }
        };

        let mut manager: ComponentMap<&str, FailArgs, Counter, _> =
            ComponentMap::try_init_async([], init).await.unwrap();

        let results: Vec<_> = manager.try_reinit_all_async().await.collect();
        assert_eq!(results.len(), 0);
    }

    #[tokio::test]
    async fn test_try_reinit_async_success() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value * 3))
                }
            }
        };

        let mut manager = ComponentMap::try_init_async(
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
        .await
        .unwrap();

        let results: Vec<_> = manager.try_reinit_async(["key1"]).await.collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].value.as_ref().unwrap().is_ok());
        assert_eq!(
            manager.components().get("key1").unwrap().component,
            Counter(3)
        );
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(6)
        );
    }

    #[tokio::test]
    async fn test_try_reinit_async_nonexistent_key() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value))
                }
            }
        };

        let mut manager = ComponentMap::try_init_async(
            [(
                "key1",
                FailArgs {
                    value: 1,
                    should_fail: false,
                },
            )],
            init,
        )
        .await
        .unwrap();

        let results: Vec<_> = manager.try_reinit_async(["nonexistent"]).await.collect();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "nonexistent");
        assert!(results[0].value.is_none());
    }

    #[tokio::test]
    async fn test_try_update_async_new_key_success() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value))
                }
            }
        };

        let mut manager = ComponentMap::try_init_async(
            [(
                "key1",
                FailArgs {
                    value: 1,
                    should_fail: false,
                },
            )],
            init,
        )
        .await
        .unwrap();

        let results: Vec<_> = manager
            .try_update_async([(
                "key2",
                FailArgs {
                    value: 20,
                    should_fail: false,
                },
            )])
            .await
            .collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].value.is_none());
        assert_eq!(manager.components().len(), 2);
        assert_eq!(
            manager.components().get("key2").unwrap().component,
            Counter(20)
        );
    }

    #[tokio::test]
    async fn test_try_update_async_failure() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value))
                }
            }
        };

        let mut manager = ComponentMap::try_init_async(
            [(
                "key1",
                FailArgs {
                    value: 1,
                    should_fail: false,
                },
            )],
            init,
        )
        .await
        .unwrap();

        let results: Vec<_> = manager
            .try_update_async([(
                "key2",
                FailArgs {
                    value: 20,
                    should_fail: true,
                },
            )])
            .await
            .collect();

        assert_eq!(results.len(), 1);
        assert!(results[0].value.is_some());
        assert!(results[0].value.as_ref().unwrap().is_err());

        // Should not insert on error
        assert_eq!(manager.components().len(), 1);
        assert!(manager.components().get("key2").is_none());
    }

    #[tokio::test]
    async fn test_try_update_async_multiple_mixed() {
        let init = |args: &FailArgs| {
            let value = args.value;
            let should_fail = args.should_fail;
            async move {
                if should_fail {
                    Err(TestError("Failed".to_string()))
                } else {
                    Ok(Counter(value))
                }
            }
        };

        let mut manager = ComponentMap::try_init_async(
            [(
                "key1",
                FailArgs {
                    value: 1,
                    should_fail: false,
                },
            )],
            init,
        )
        .await
        .unwrap();

        let results: Vec<_> = manager
            .try_update_async([
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
            .await
            .collect();

        assert_eq!(results.len(), 3);

        // Check that only successful updates were inserted
        assert_eq!(manager.components().len(), 3); // key1, key2, key4
        assert!(manager.components().get("key2").is_some());
        assert!(manager.components().get("key3").is_none());
        assert!(manager.components().get("key4").is_some());
    }
}
