use derive_more::Constructor;
use std::collections::HashMap;

mod async_fallible;
mod async_infallible;
mod sync_fallible;
mod sync_infallible;

#[derive(Debug, Constructor)]
pub struct Keyed<Key, Value> {
    key: Key,
    value: Value,
}

#[derive(Debug, Constructor)]
pub struct WithArgs<Args, Comp> {
    pub component: Comp,
    pub args: Args,
}

#[derive(Debug, Constructor)]
pub struct ComponentMap<Key, Args, Comp, FnInit> {
    pub map: HashMap<Key, WithArgs<Args, Comp>>,
    pub init: FnInit,
}
