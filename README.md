# component-map

Manage keyed components and their initialisation arguments, then replace them
as configuration changes. Supports sync/async and fallible/infallible factories.

```rust
use component_map::ComponentMap;

fn main() -> Result<(), std::num::ParseIntError> {
    let init = |_key: &&str, args: &String| args.parse::<u32>();
    let mut components = ComponentMap::try_init([("limit", "10".into())], init)?;

    let previous = components.try_update_one("limit", "20".into())?;
    assert_eq!(previous.unwrap().component, 10);
    assert_eq!(components.map["limit"].component, 20);
    Ok(())
}
```

Failed updates retain the current component and arguments. Capture shared
dependencies in the factory; handle returned components to retire old resources.

For streamed configuration, see [live_config](examples/live_config.rs).

MIT licensed.
