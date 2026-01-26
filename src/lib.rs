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
pub struct ComponentManager<Key, Args, Comp, FnInit> {
    map: HashMap<Key, WithArgs<Args, Comp>>,
    init: FnInit,
}

impl<Key, Args, Comp, FnInit> ComponentManager<Key, Args, Comp, FnInit> {
    pub fn components(&self) -> &HashMap<Key, WithArgs<Args, Comp>> {
        &self.map
    }

    pub fn components_mut(&mut self) -> &mut HashMap<Key, WithArgs<Args, Comp>> {
        &mut self.map
    }

    pub fn fn_init(&self) -> &FnInit {
        &self.init
    }
}
