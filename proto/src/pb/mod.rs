//! Protobuf types for the packages of this repository, generated from
//! `proto/v1` by the `buf.build/anthropics/buffa` plugin (v0.9.2, see
//! `proto/buf.gen.yaml`); run `buf generate` in `proto/` to regenerate. This
//! file is written by hand.

macro_rules! include_buffa {
    ($package:literal) => {
        include!(concat!($package, ".mod.rs"));
    };
}

pub mod aerodrome {
    pub mod v1 {
        include_buffa!("aerodrome.v1");
    }
}
pub mod balancer {
    pub mod v1 {
        include_buffa!("balancer.v1");
    }
}
pub mod bancor {
    pub mod v1 {
        include_buffa!("bancor.v1");
    }
}
pub mod cow {
    pub mod v1 {
        include_buffa!("cow.v1");
    }
}
pub mod curvefi {
    pub mod v1 {
        include_buffa!("curvefi.v1");
    }
}
pub mod dodo {
    pub mod v1 {
        include_buffa!("dodo.v1");
    }
}
pub mod kyber_elastic {
    pub mod v1 {
        include_buffa!("kyber_elastic.v1");
    }
}
pub mod sunpump {
    pub mod v1 {
        include_buffa!("sunpump.v1");
    }
}
pub mod traderjoe {
    pub mod v1 {
        include_buffa!("traderjoe.v1");
    }
}
/// `uniswap.v2` and `uniswap.v3` each hold two files: the raw events of
/// `dex/uniswap-v2` and `dex/uniswap-v3` and the pool state of
/// `dex/pool-state`. Their message names are disjoint.
pub mod uniswap {
    pub mod v1 {
        include_buffa!("uniswap.v1");
    }
    pub mod v2 {
        include_buffa!("uniswap.v2");
    }
    pub mod v3 {
        include_buffa!("uniswap.v3");
    }
    pub mod v4 {
        include_buffa!("uniswap.v4");
    }
}
pub mod woofi {
    pub mod v1 {
        include_buffa!("woofi.v1");
    }
}
pub mod dex {
    pub mod foundational_store {
        pub mod v1 {
            include_buffa!("dex.foundational_store.v1");
        }
    }
    pub mod pool_state {
        pub mod v1 {
            include_buffa!("dex.pool_state.v1");
        }
    }
    pub mod swaps {
        pub mod v1 {
            include_buffa!("dex.swaps.v1");
        }
    }
}
