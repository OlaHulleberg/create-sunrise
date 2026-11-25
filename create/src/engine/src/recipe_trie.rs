//! Create's recipe trie (`RecipeTrie`, `IntArrayTrie`): recipes by the ingredients they take,
//! for the lookup of every recipe a basin's contents can make, in the order
//! `BasinOperatingBlockEntity.getMatchingRecipes` hands them out. The lookup follows the
//! iteration order of the tables its Java sits on, fastutil's `IntOpenHashSet` and
//! `Int2ObjectOpenHashMap`, so those are here too.

use crate::tank::Fluid;
use std::collections::HashMap;

/// `HashCommon.mix`.
fn mix(key: i32) -> u32 {
    let h = key.wrapping_mul(0x9E37_79B9u32 as i32) as u32;
    h ^ (h >> 16)
}

/// `HashCommon.nextPowerOfTwo`.
fn next_power_of_two(x: u64) -> u64 {
    1 << (64 - (x - 1).leading_zeros())
}

/// `HashCommon.arraySize`, at the load factor three quarters.
fn array_size(expected: usize) -> usize {
    next_power_of_two((expected as f32 / 0.75).ceil() as u64) as usize
}

/// `HashCommon.maxFill`, at the same.
fn max_fill(n: usize) -> usize {
    ((n as f32 * 0.75).ceil() as usize).min(n - 1)
}

/// What fastutil's `IntOpenHashSet` and `Int2ObjectOpenHashMap` are laid out as: an open table
/// of int keys, the key 0 kept aside, linear probing on from `HashCommon.mix(key) & mask`, at
/// most three quarters full (then grown, the keys placed again from the table's end back).
#[derive(Clone, Debug)]
struct IntTable<V> {
    /// The keys of the `n` slots and of the key 0's own; 0 in the first `n` is empty.
    key: Vec<i32>,
    value: Vec<Option<V>>,
    n: usize,
    mask: usize,
    max_fill: usize,
    size: usize,
    contains_null: bool,
}

impl<V> IntTable<V> {
    fn new() -> IntTable<V> {
        IntTable {
            key: vec![0; 33],
            value: (0..33).map(|_| None).collect(),
            n: 32,
            mask: 31,
            max_fill: max_fill(32),
            size: 0,
            contains_null: false,
        }
    }

    /// `realSize`: the entries outside the slot of the key 0.
    fn real_size(&self) -> usize {
        self.size - usize::from(self.contains_null)
    }

    /// `find`: the slot of `key`.
    fn find(&self, key: i32) -> Option<usize> {
        if key == 0 {
            return self.contains_null.then_some(self.n);
        }
        let mut pos = (mix(key) & self.mask as u32) as usize;
        loop {
            match self.key[pos] {
                0 => return None,
                held if held == key => return Some(pos),
                _ => pos = (pos + 1) & self.mask,
            }
        }
    }

    /// `insert`: the slot of `key`, its value `init`'s if the key is new; and whether it was.
    fn insert(&mut self, key: i32, init: impl FnOnce() -> V) -> (usize, bool) {
        if let Some(pos) = self.find(key) {
            return (pos, false);
        }
        let value = Some(init());
        if key == 0 {
            self.contains_null = true;
            self.value[self.n] = value;
        } else {
            let mut pos = (mix(key) & self.mask as u32) as usize;
            while self.key[pos] != 0 {
                pos = (pos + 1) & self.mask;
            }
            self.key[pos] = key;
            self.value[pos] = value;
        }
        self.size += 1;
        if self.size >= self.max_fill {
            self.rehash(array_size(self.size + 1));
        }
        (self.find(key).expect("just inserted"), true)
    }

    /// `rehash(newN)`: the keys placed into a table of `new_n` slots, from the old table's end
    /// back.
    fn rehash(&mut self, new_n: usize) {
        let mask = new_n - 1;
        let mut key = vec![0; new_n + 1];
        let mut value: Vec<Option<V>> = (0..new_n + 1).map(|_| None).collect();
        let mut i = self.n;
        for _ in 0..self.real_size() {
            while {
                i -= 1;
                self.key[i] == 0
            } {}
            let k = self.key[i];
            let mut pos = (mix(k) & mask as u32) as usize;
            while key[pos] != 0 {
                pos = (pos + 1) & mask;
            }
            key[pos] = k;
            value[pos] = self.value[i].take();
        }
        value[new_n] = self.value[self.n].take();
        self.key = key;
        self.value = value;
        self.n = new_n;
        self.mask = mask;
        self.max_fill = max_fill(new_n);
    }

    /// `tryCapacity`: grown to hold `capacity` entries, if that takes a bigger table.
    fn try_capacity(&mut self, capacity: usize) {
        let needed = next_power_of_two((capacity as f32 / 0.75).ceil() as u64).max(2) as usize;
        if needed > self.n {
            self.rehash(needed);
        }
    }

    /// The slots in fastutil's iteration order: the key 0 first, then the table from its end.
    fn slots(&self) -> impl Iterator<Item = usize> + '_ {
        self.contains_null
            .then_some(self.n)
            .into_iter()
            .chain((0..self.n).rev().filter(|&i| self.key[i] != 0))
    }
}

/// `IntOpenHashSet`.
#[derive(Clone, Debug)]
pub struct IntOpenHashSet {
    table: IntTable<()>,
}

impl Default for IntOpenHashSet {
    fn default() -> Self {
        IntOpenHashSet::new()
    }
}

impl IntOpenHashSet {
    pub fn new() -> IntOpenHashSet {
        IntOpenHashSet {
            table: IntTable::new(),
        }
    }

    /// `size`.
    pub fn len(&self) -> usize {
        self.table.size
    }

    /// `isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.table.size == 0
    }

    /// `add`: whether `key` was new.
    pub fn add(&mut self, key: i32) -> bool {
        self.table.insert(key, || ()).1
    }

    /// `contains`.
    pub fn contains(&self, key: i32) -> bool {
        self.table.find(key).is_some()
    }

    /// `addAll`: grown to hold both sets first (`tryCapacity`), then each of `other`'s keys in
    /// its iteration order.
    pub fn add_all(&mut self, other: &IntOpenHashSet) {
        self.table.try_capacity(self.table.size + other.table.size);
        for key in other.keys() {
            self.add(key);
        }
    }

    /// The keys in fastutil's iteration order.
    pub fn keys<'a>(&'a self) -> impl Iterator<Item = i32> + 'a {
        self.table.slots().map(|i| self.table.key[i])
    }
}

/// `Int2ObjectOpenHashMap`.
#[derive(Clone, Debug)]
pub struct Int2ObjectOpenHashMap<V> {
    table: IntTable<V>,
}

impl<V> Default for Int2ObjectOpenHashMap<V> {
    fn default() -> Self {
        Int2ObjectOpenHashMap::new()
    }
}

impl<V> Int2ObjectOpenHashMap<V> {
    pub fn new() -> Int2ObjectOpenHashMap<V> {
        Int2ObjectOpenHashMap {
            table: IntTable::new(),
        }
    }

    /// `size`.
    pub fn len(&self) -> usize {
        self.table.size
    }

    /// `isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.table.size == 0
    }

    /// `get`.
    pub fn get(&self, key: i32) -> Option<&V> {
        self.table
            .find(key)
            .and_then(|i| self.table.value[i].as_ref())
    }

    /// The value of `key`, to change.
    pub fn get_mut(&mut self, key: i32) -> Option<&mut V> {
        let pos = self.table.find(key)?;
        self.table.value[pos].as_mut()
    }

    /// `computeIfAbsent`: the value of `key`, `init`'s if there was none.
    pub fn compute_if_absent(&mut self, key: i32, init: impl FnOnce() -> V) -> &mut V {
        let (pos, _) = self.table.insert(key, init);
        self.table.value[pos].as_mut().expect("a value per key")
    }

    /// `int2ObjectEntrySet` in fastutil's iteration order.
    pub fn entries<'a>(&'a self) -> impl Iterator<Item = (i32, &'a V)> + 'a {
        self.table.slots().filter_map(|i| {
            let value = self.table.value[i].as_ref()?;
            Some((self.table.key[i], value))
        })
    }
}

/// `IntArrayTrie<V>`: a sorted int key to many values, with a lookup of every value whose key is
/// a subset of a pool.
#[derive(Clone, Debug)]
pub struct IntArrayTrie<V> {
    root: Node<V>,
    max_depth: usize,
    node_count: usize,
    value_count: usize,
}

#[derive(Clone, Debug)]
struct Node<V> {
    /// The next key of each deeper path, and the node it leads to.
    children: Int2ObjectOpenHashMap<Box<Node<V>>>,
    values: Vec<V>,
}

impl<V> Default for Node<V> {
    fn default() -> Self {
        Node {
            children: Int2ObjectOpenHashMap::new(),
            values: Vec::new(),
        }
    }
}

impl<V> Default for IntArrayTrie<V> {
    fn default() -> Self {
        IntArrayTrie {
            root: Node::default(),
            max_depth: 0,
            node_count: 1,
            value_count: 0,
        }
    }
}

impl<V> IntArrayTrie<V> {
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    pub fn node_count(&self) -> usize {
        self.node_count
    }

    pub fn value_count(&self) -> usize {
        self.value_count
    }

    /// `insert`: a sorted `key`, its `value` at the node the key ends at.
    pub fn insert(&mut self, key: &[i32], value: V) {
        let mut node = &mut self.root;
        for &k in key {
            let children = &mut node.children;
            let before = children.len();
            children.compute_if_absent(k, Box::default);
            self.node_count += usize::from(children.len() > before);
            node = &mut *children.get_mut(k).expect("just added");
        }
        node.values.push(value);
        self.max_depth = self.max_depth.max(key.len());
        self.value_count += 1;
    }

    /// `lookup`: every value whose key is a subset of `pool`, in the traversal's order.
    pub fn lookup<'a>(&'a self, pool: &IntOpenHashSet) -> Vec<&'a V> {
        let mut out = Vec::new();
        self.dfs(&self.root, pool, &mut out);
        out
    }

    /// `dfs`: a node's values, then those under each child whose key the pool has. With more
    /// children than the pool has keys the pool is walked, else the children: the shorter.
    fn dfs<'a>(&'a self, node: &'a Node<V>, pool: &IntOpenHashSet, out: &mut Vec<&'a V>) {
        out.extend(node.values.iter());
        if node.children.len() > pool.len() {
            for key in pool.keys() {
                if let Some(child) = node.children.get(key) {
                    self.dfs(child, pool, out);
                }
            }
        } else {
            for (key, child) in node.children.entries() {
                if pool.contains(key) {
                    self.dfs(child, pool, out);
                }
            }
        }
    }
}

/// `AbstractVariant`: an item or a fluid, what a basin can hold.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Variant {
    Item(String),
    Fluid(Fluid),
}

/// `AbstractIngredient`: the variants an ingredient takes, or the universal one, which any
/// contents satisfy (an empty ingredient).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AbstractIngredient {
    Universal,
    Variants(Vec<Variant>),
}

/// `RecipeTrie<R>`: the recipes of a kind by the `AbstractIngredient`s they take.
#[derive(Clone, Debug)]
pub struct RecipeTrie<R> {
    trie: IntArrayTrie<R>,
    variant_to_id: HashMap<Variant, i32>,
    variant_to_ingredients: Int2ObjectOpenHashMap<IntOpenHashSet>,
    universal_ingredient_id: i32,
}

/// `RecipeTrie.Builder`.
#[derive(Clone, Debug)]
pub struct RecipeTrieBuilder<R> {
    trie: IntArrayTrie<R>,
    variant_to_id: HashMap<Variant, i32>,
    variant_to_ingredients: Int2ObjectOpenHashMap<IntOpenHashSet>,
    universal_ingredient_id: i32,
    ingredient_to_id: HashMap<AbstractIngredient, i32>,
}

impl<R> Default for RecipeTrieBuilder<R> {
    fn default() -> Self {
        let mut builder = RecipeTrieBuilder {
            trie: IntArrayTrie::default(),
            variant_to_id: HashMap::new(),
            variant_to_ingredients: Int2ObjectOpenHashMap::new(),
            universal_ingredient_id: 0,
            ingredient_to_id: HashMap::new(),
        };
        builder.universal_ingredient_id = builder.ingredient_id(&AbstractIngredient::Universal);
        builder
    }
}

impl<R> RecipeTrieBuilder<R> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `getOrAssignId(variant)`.
    fn variant_id(&mut self, variant: &Variant) -> i32 {
        let next = self.variant_to_id.len() as i32;
        *self.variant_to_id.entry(variant.clone()).or_insert(next)
    }

    /// `getOrAssignId(ingredient)`: its id; each of its variants lists it among the ingredients
    /// it satisfies.
    fn ingredient_id(&mut self, ingredient: &AbstractIngredient) -> i32 {
        if let Some(&id) = self.ingredient_to_id.get(ingredient) {
            return id;
        }
        let id = self.ingredient_to_id.len() as i32;
        self.ingredient_to_id.insert(ingredient.clone(), id);
        if let AbstractIngredient::Variants(variants) = ingredient {
            for variant in variants {
                let variant = self.variant_id(variant);
                self.variant_to_ingredients
                    .compute_if_absent(variant, IntOpenHashSet::new)
                    .add(id);
            }
        }
        id
    }

    /// `insert(recipe)`: `ingredients` are the recipe's distinct `AbstractIngredient`s, as
    /// `createRecipe` makes them.
    pub fn insert(&mut self, recipe: R, ingredients: &[AbstractIngredient]) {
        let mut key: Vec<i32> = ingredients
            .iter()
            .map(|ingredient| self.ingredient_id(ingredient))
            .collect();
        key.sort_unstable();
        self.trie.insert(&key, recipe);
    }

    /// `build`.
    pub fn build(self) -> RecipeTrie<R> {
        RecipeTrie {
            trie: self.trie,
            variant_to_id: self.variant_to_id,
            variant_to_ingredients: self.variant_to_ingredients,
            universal_ingredient_id: self.universal_ingredient_id,
        }
    }
}

impl<R> RecipeTrie<R> {
    /// `lookup(pool)`: every recipe whose ingredients `pool` can satisfy, in the trie's
    /// traversal's order. `pool` is what a basin holds, in the order its slots and tanks name
    /// it (Java walks a `HashSet`).
    pub fn lookup<'a>(&'a self, pool: &[Variant]) -> Vec<&'a R> {
        let mut ingredients = IntOpenHashSet::new();
        ingredients.add(self.universal_ingredient_id);
        for variant in pool {
            if let Some(&id) = self.variant_to_id.get(variant)
                && let Some(ids) = self.variant_to_ingredients.get(id)
            {
                ingredients.add_all(ids);
            }
        }
        self.trie.lookup(&ingredients)
    }

    /// Depth, nodes, variants and recipes, as the builder logs them.
    pub fn stats(&self) -> (usize, usize, usize, usize) {
        (
            self.trie.max_depth(),
            self.trie.node_count(),
            self.variant_to_id.len(),
            self.trie.value_count(),
        )
    }
}

/// The end of `BasinOperatingBlockEntity.getMatchingRecipes`: of the recipes a lookup handed
/// out (`trie.lookup`, then what a machine adds), those `fits` keeps (`matchBasinRecipe`),
/// sorted stably by the number of item ingredients `count` gives, the most first. Fluid
/// ingredients do not count.
pub fn get_matching_recipes<R>(
    handed_out: impl IntoIterator<Item = R>,
    fits: impl Fn(&R) -> bool,
    count: impl Fn(&R) -> usize,
) -> Vec<R> {
    let mut list: Vec<R> = handed_out.into_iter().filter(|r| fits(r)).collect();
    list.sort_by_key(|r| std::cmp::Reverse(count(r)));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(keys: &[i32]) -> IntOpenHashSet {
        let mut set = IntOpenHashSet::new();
        for &k in keys {
            set.add(k);
        }
        set
    }

    /// fastutil 8.5's order for 1..10 in a fresh table: slots by `HashCommon.mix(key) & 31`,
    /// walked from the table's end (28, 26, 25, 19, 18, 16, 14, 13, 11, 8).
    #[test]
    fn a_set_iterates_from_its_table_s_end() {
        assert_eq!(
            (1..=10).map(|k| mix(k) & 31).collect::<Vec<_>>(),
            [14, 28, 13, 25, 8, 26, 11, 19, 18, 16]
        );
        assert_eq!(
            set(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
                .keys()
                .collect::<Vec<_>>(),
            [2, 6, 4, 8, 9, 10, 1, 3, 7, 5]
        );
        assert_eq!(
            set(&[0, 1, 2, 3, 5, 8, 13, 21, 34, 55])
                .keys()
                .collect::<Vec<_>>(),
            [0, 2, 55, 13, 8, 1, 3, 34, 5, 21]
        );
    }

    /// The 25th key grows the table to 64 slots, the keys placed again from the old end back.
    #[test]
    fn a_set_grows_at_three_quarters() {
        let set = set(&(1..=26).collect::<Vec<_>>());
        assert_eq!(set.len(), 26);
        assert_eq!(set.table.n, 64);
        assert_eq!(
            set.keys().collect::<Vec<_>>(),
            [
                25, 4, 12, 13, 8, 9, 24, 26, 18, 16, 17, 19, 23, 20, 22, 21, 2, 6, 15, 14, 11, 10,
                1, 3, 7, 5
            ]
        );
    }

    /// `addAll` grows the table for both first (`tryCapacity`), then adds in the other's order.
    #[test]
    fn add_all_takes_the_other_s_order() {
        let mut a = set(&[1, 4, 9]);
        a.add_all(&set(&[16, 25, 36]));
        assert_eq!(a.keys().collect::<Vec<_>>(), [25, 4, 9, 1, 36, 16]);
        assert!(a.contains(16) && !a.contains(2));
    }

    /// A map's entries come in the order of its keys' slots, the key 0 first.
    #[test]
    fn a_map_s_entries_follow_its_keys() {
        let mut map = Int2ObjectOpenHashMap::new();
        for k in [5, 0, 3, 9] {
            *map.compute_if_absent(k, || k * 10) += 1;
        }
        assert_eq!(map.get(0), Some(&1));
        assert_eq!(map.get(5), Some(&51));
        assert_eq!(map.len(), 4);
        assert_eq!(
            map.entries().map(|(k, v)| (k, *v)).collect::<Vec<_>>(),
            [(0, 1), (9, 91), (3, 31), (5, 51)]
        );
    }

    /// `BasinOperatingBlockEntity.getMatchingRecipes`: `create:mixing/mud_by_mixing` (dirt and
    /// water: one item ingredient) and `minecraft:packed_mud` (mud and wheat: two) both fit a
    /// basin of dirt, water, mud and wheat. The trie hands out in the order of its keys' slots
    /// (ingredient ids by first appearance: the mixing recipe first when inserted first, the
    /// packing one when it was); the sort puts the most item ingredients first either way, and
    /// among equals keeps the order handed out.
    #[test]
    fn the_most_item_ingredients_comes_first_whatever_the_trie_s_order() {
        let ingredient = |items: &[&str]| {
            AbstractIngredient::Variants(items.iter().map(|i| Variant::Item((*i).into())).collect())
        };
        let water = AbstractIngredient::Variants(vec![Variant::Fluid(Fluid::Water)]);
        let mud_by_mixing = (
            "create:mixing/mud_by_mixing",
            vec![ingredient(&["minecraft:dirt"]), water.clone()],
        );
        let packed_mud = (
            "minecraft:packed_mud",
            vec![
                ingredient(&["minecraft:mud"]),
                ingredient(&["minecraft:wheat"]),
            ],
        );
        let pool = [
            Variant::Item("minecraft:dirt".into()),
            Variant::Fluid(Fluid::Water),
            Variant::Item("minecraft:mud".into()),
            Variant::Item("minecraft:wheat".into()),
        ];
        let fits = |_: &&str| true;
        let count = |r: &&str| if *r == "minecraft:packed_mud" { 2 } else { 1 };
        let sorted = ["minecraft:packed_mud", "create:mixing/mud_by_mixing"];
        for (order, lookup) in [
            (
                [mud_by_mixing.clone(), packed_mud.clone()],
                ["create:mixing/mud_by_mixing", "minecraft:packed_mud"],
            ),
            (
                [packed_mud.clone(), mud_by_mixing.clone()],
                ["minecraft:packed_mud", "create:mixing/mud_by_mixing"],
            ),
        ] {
            let mut builder = RecipeTrieBuilder::new();
            for (id, ingredients) in &order {
                builder.insert(*id, ingredients);
            }
            let trie = builder.build();
            assert_eq!(trie.stats(), (2, 5, 4, 2));
            let handed_out: Vec<&str> = trie.lookup(&pool).into_iter().copied().collect();
            assert_eq!(handed_out, lookup);
            assert_eq!(get_matching_recipes(handed_out, fits, count), sorted);
        }
        let mut builder = RecipeTrieBuilder::new();
        builder.insert(
            "b",
            &[ingredient(&["minecraft:x"]), ingredient(&["minecraft:y"])],
        );
        builder.insert(
            "a",
            &[ingredient(&["minecraft:z"]), ingredient(&["minecraft:w"])],
        );
        let trie = builder.build();
        let pool = ["x", "y", "z", "w"].map(|i| Variant::Item(format!("minecraft:{i}")));
        let handed_out: Vec<&str> = trie.lookup(&pool).into_iter().copied().collect();
        assert_eq!(handed_out, ["b", "a"]);
        assert_eq!(get_matching_recipes(handed_out, fits, |_| 2), ["b", "a"]);
        let mut builder = RecipeTrieBuilder::new();
        builder.insert(
            "c",
            &[
                ingredient(&["minecraft:x"]),
                ingredient(&["minecraft:y"]),
                ingredient(&["minecraft:z"]),
            ],
        );
        let all = builder.build();
        let handed_out: Vec<&str> = all
            .lookup(&pool)
            .into_iter()
            .copied()
            .chain(["b", "a"])
            .collect();
        assert_eq!(
            get_matching_recipes(handed_out, fits, |r| match *r {
                "c" => 3,
                _ => 2,
            }),
            ["c", "b", "a"]
        );
    }
}
