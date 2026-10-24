//! Bounded, offline host for the immutable v0.8.0 pool/swap/row graph.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wasmi::{Caller, Config, Engine, Error, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, Val};

pub type Pools = BTreeMap<String, Vec<u8>>;
const IO_LIMIT: usize = 4 * 1024 * 1024;
const MAX_POOLS: usize = 4096;
const MAX_POOL_BYTES: usize = 4096;
struct Host {
    limits: StoreLimits,
    pools: Pools,
    calls: usize,
    writable: bool,
    output: Option<Vec<u8>>,
    panicked: bool,
}
pub struct Run {
    pub output: Vec<u8>,
    pub pools: Pools,
}
pub struct Oracle {
    engine: Engine,
    pools: Module,
    swaps: Module,
    rows: Module,
}
fn memory(caller: &Caller<'_, Host>) -> Result<Memory, Error> {
    caller
        .get_export("memory")
        .and_then(|value| value.into_memory())
        .ok_or_else(|| Error::new("missing memory"))
}
fn read(caller: &Caller<'_, Host>, pointer: i32, length: i32, limit: usize) -> Result<Vec<u8>, Error> {
    let pointer = usize::try_from(pointer).map_err(|_| Error::new("negative pointer"))?;
    let length = usize::try_from(length).map_err(|_| Error::new("negative length"))?;
    if length > limit {
        return Err(Error::new("oversized host read"));
    }
    let mut bytes = vec![0; length];
    memory(caller)?
        .read(caller, pointer, &mut bytes)
        .map_err(|_| Error::new("invalid memory range"))?;
    Ok(bytes)
}
fn key(caller: &Caller<'_, Host>, pointer: i32, length: i32) -> Result<String, Error> {
    let key = String::from_utf8(read(caller, pointer, length, 64)?).map_err(|_| Error::new("invalid key"))?;
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err(Error::new("invalid hex key"));
    }
    Ok(key)
}
fn count(caller: &mut Caller<'_, Host>) -> Result<(), Error> {
    caller.data_mut().calls += 1;
    if caller.data().calls > 16_384 {
        return Err(Error::new("store call bound exceeded"));
    }
    Ok(())
}
impl Oracle {
    pub fn new() -> Self {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = |bytes: &[u8], hash: &str| {
            assert_eq!(format!("{:x}", Sha256::digest(bytes)), hash);
            Module::new(&engine, bytes).unwrap()
        };
        let pools = module(
            include_bytes!("../fixtures/legacy-v0.8.0/store_pools.wasm"),
            "0b71ff6162bce3fe6da4b29aea2333ac957bc10325405866b4de1dd0935be658",
        );
        let swaps = module(
            include_bytes!("../fixtures/legacy-v0.8.0/dex_swaps.wasm"),
            "bbc351be2e79ab90e9a265dc07370804ec63f1fc82150c0bbbd35851e7455016",
        );
        let rows = module(
            include_bytes!("../fixtures/legacy-v0.8.0/db_out.wasm"),
            "6e3fb029b9511fac79a732f391099521ab6e560b5519de62f3ec7621208dabe4",
        );
        Self { engine, pools, swaps, rows }
    }
    pub fn store(&self, block: &[u8], seed: Pools) -> Option<Run> {
        self.run(&self.pools, "store_pools", &[block], seed, true, false)
    }
    pub fn swaps(&self, block: &[u8], pools: Pools) -> Option<Run> {
        self.run(&self.swaps, "map_events", &[block], pools, false, true)
    }
    pub fn rows(&self, clock: &[u8], events: &[u8], encoding: &[u8]) -> Option<Run> {
        self.run(&self.rows, "db_out", &[encoding, clock, events], Pools::new(), false, false)
    }
    fn run(&self, module: &Module, function: &str, inputs: &[&[u8]], pools: Pools, writable: bool, store_input: bool) -> Option<Run> {
        assert!(pools.len() <= MAX_POOLS);
        assert!(pools.iter().all(|(key, value)| !key.is_empty()
            && key.len() <= 64
            && key.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            && value.len() <= MAX_POOL_BYTES));
        let mut store = Store::new(
            &self.engine,
            Host {
                limits: StoreLimitsBuilder::new().memory_size(64 * 1024 * 1024).instances(1).memories(1).build(),
                pools,
                calls: 0,
                writable,
                output: None,
                panicked: false,
            },
        );
        store.limiter(|host| &mut host.limits);
        store.set_fuel(1_000_000_000).unwrap();
        let mut linker = Linker::<Host>::new(&self.engine);
        linker.func_wrap("logger", "println", |_: i32, _: i32| {}).unwrap();
        linker.func_wrap("env", "skip_empty_output", || {}).unwrap();
        linker
            .func_wrap(
                "env",
                "register_panic",
                |mut caller: Caller<'_, Host>, _: i32, _: i32, _: i32, _: i32, _: i32, _: i32| {
                    caller.data_mut().panicked = true;
                },
            )
            .unwrap();
        linker
            .func_wrap("env", "output", |mut caller: Caller<'_, Host>, ptr: i32, len: i32| -> Result<(), Error> {
                if caller.data().output.is_some() || caller.data().writable {
                    return Err(Error::new("unexpected output"));
                }
                let bytes = read(&caller, ptr, len, IO_LIMIT)?;
                caller.data_mut().output = Some(bytes);
                Ok(())
            })
            .unwrap();
        linker
            .func_wrap(
                "state",
                "get_last",
                |mut caller: Caller<'_, Host>, index: i32, ptr: i32, len: i32, out: i32| -> Result<i32, Error> {
                    count(&mut caller)?;
                    if index != 0 || caller.data().writable {
                        return Err(Error::new("unexpected store read"));
                    }
                    let key = key(&caller, ptr, len)?;
                    let Some(value) = caller.data().pools.get(&key).cloned() else { return Ok(0) };
                    let allocate = caller
                        .get_export("alloc")
                        .and_then(|value| value.into_func())
                        .ok_or_else(|| Error::new("missing allocator"))?
                        .typed::<i32, i32>(&caller)?;
                    let ptr = allocate.call(&mut caller, i32::try_from(value.len()).unwrap())?;
                    let mem = memory(&caller)?;
                    mem.write(&mut caller, usize::try_from(ptr).map_err(|_| Error::new("negative allocation"))?, &value)
                        .map_err(|_| Error::new("invalid allocation"))?;
                    let mut pair = ptr.to_le_bytes().to_vec();
                    pair.extend(u32::try_from(value.len()).unwrap().to_le_bytes());
                    mem.write(&mut caller, usize::try_from(out).map_err(|_| Error::new("negative result pointer"))?, &pair)
                        .map_err(|_| Error::new("invalid result pointer"))?;
                    Ok(1)
                },
            )
            .unwrap();
        linker
            .func_wrap(
                "state",
                "set_if_not_exists",
                |mut caller: Caller<'_, Host>, _ordinal: i64, ptr: i32, len: i32, value_ptr: i32, value_len: i32| -> Result<(), Error> {
                    count(&mut caller)?;
                    if !caller.data().writable {
                        return Err(Error::new("unexpected store write"));
                    }
                    let key = key(&caller, ptr, len)?;
                    let value = read(&caller, value_ptr, value_len, MAX_POOL_BYTES)?;
                    if !caller.data().pools.contains_key(&key) && caller.data().pools.len() >= MAX_POOLS {
                        return Err(Error::new("pool capacity exceeded"));
                    }
                    caller.data_mut().pools.entry(key).or_insert(value);
                    Ok(())
                },
            )
            .unwrap();
        // Every import must match an explicit, bounded SDK call above.
        let instance = linker.instantiate(&mut store, module).unwrap().start(&mut store).unwrap();
        let allocate = instance.get_typed_func::<i32, i32>(&store, "alloc").unwrap();
        let memory = instance.get_memory(&store, "memory").unwrap();
        let mut arguments = Vec::new();
        for input in inputs {
            assert!(input.len() <= IO_LIMIT);
            let len = i32::try_from(input.len()).unwrap();
            let ptr = allocate.call(&mut store, len).unwrap();
            memory.write(&mut store, usize::try_from(ptr).unwrap(), input).unwrap();
            arguments.extend([Val::I32(ptr), Val::I32(len)]);
        }
        if store_input {
            arguments.push(Val::I32(0));
        }
        let result = instance.get_func(&store, function).unwrap().call(&mut store, &arguments, &mut []);
        if store.data().panicked {
            assert!(result.is_err(), "registered panic must trap");
            assert!(store.data().output.is_none(), "panic cannot publish");
            None
        } else {
            result.unwrap_or_else(|e| panic!("unexpected {function} host/trap/fuel failure: {e}"));
            let host = store.into_data();
            Some(Run {
                output: host.output.unwrap_or_default(),
                pools: host.pools,
            })
        }
    }
}
