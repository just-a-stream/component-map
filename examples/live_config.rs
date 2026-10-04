use component_map::ComponentMap;
use futures::{Stream, StreamExt, executor::block_on, stream};
use std::{collections::HashMap, pin::pin};

fn main() {
    let updates = stream::iter([
        ("limit", "10".to_owned()),
        ("limit", "invalid".to_owned()),
        ("limit", "20".to_owned()),
    ]);
    let limits = block_on(run(updates));

    assert_eq!(limits["limit"], 20);
    println!("applied limit: {}", limits["limit"]);
}

async fn run(updates: impl Stream<Item = (&'static str, String)>) -> HashMap<&'static str, u32> {
    let init = |_key: &&str, args: &String| args.parse::<u32>();
    let mut components = ComponentMap::new(HashMap::new(), init);
    let mut updates = pin!(updates);

    while let Some((key, args)) = updates.next().await {
        match components.try_update_one(key, args) {
            Ok(previous) => drop(previous),
            Err(error) => eprintln!("rejected update for {key}: {error}"),
        }
    }

    components
        .map
        .into_iter()
        .map(|(key, entry)| (key, entry.component))
        .collect()
}
