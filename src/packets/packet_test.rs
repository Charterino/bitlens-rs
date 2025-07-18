use std::alloc::System;

use bumpalo::Bump;
use supercow::Supercow;
use tracking_allocator::{
    AllocationGroupId, AllocationGroupToken, AllocationRegistry, AllocationTracker, Allocator,
};

use crate::packets::block::Block;

use super::packetpayload::Serializable;

// This is where we actually set the global allocator to be the shim allocator implementation from `tracking_allocator`.
// This allocator is purely a facade to the logic provided by the crate, which is controlled by setting a global tracker
// and registering allocation groups.  All of that is covered below.
//
// As well, you can see here that we're wrapping the system allocator.  If you want, you can construct `Allocator` by
// wrapping another allocator that implements `GlobalAlloc`.  Since this is a static, you need a way to construct ther
// allocator to be wrapped in a const fashion, but it _is_ possible.
#[global_allocator]
static GLOBAL: Allocator<System> = Allocator::system();

struct StdoutTracker;

// This is our tracker implementation.  You will always need to create an implementation of `AllocationTracker` in order
// to actually handle allocation events.  The interface is straightforward: you're notified when an allocation occurs,
// and when a deallocation occurs.
impl AllocationTracker for StdoutTracker {
    fn allocated(
        &self,
        addr: usize,
        object_size: usize,
        wrapped_size: usize,
        group_id: AllocationGroupId,
    ) {
        // Allocations have all the pertinent information upfront, which you may or may not want to store for further
        // analysis. Notably, deallocations also know how large they are, and what group ID they came from, so you
        // typically don't have to store much data for correlating deallocations with their original allocation.
        println!(
            "allocation -> addr=0x{:0x} object_size={} wrapped_size={} group_id={:?}",
            addr, object_size, wrapped_size, group_id
        );
    }

    fn deallocated(
        &self,
        addr: usize,
        object_size: usize,
        wrapped_size: usize,
        source_group_id: AllocationGroupId,
        current_group_id: AllocationGroupId,
    ) {
        // When a deallocation occurs, as mentioned above, you have full access to the address, size of the allocation,
        // as well as the group ID the allocation was made under _and_ the active allocation group ID.
        //
        // This can be useful beyond just the obvious "track how many current bytes are allocated by the group", instead
        // going further to see the chain of where allocations end up, and so on.
        println!(
            "deallocation -> addr=0x{:0x} object_size={} wrapped_size={} source_group_id={:?} current_group_id={:?}",
            addr, object_size, wrapped_size, source_group_id, current_group_id
        );
    }
}

#[test]
fn test_deserialize_safety() {
    AllocationRegistry::set_global_tracker(StdoutTracker)
        .expect("no other global tracker should be set yet");
    let mut token = AllocationGroupToken::register().unwrap();

    let raw_block =
        hex::decode("paste huge block data here, didn't want to push 2mb to git.").unwrap();
    AllocationRegistry::enable_tracking();
    let alloc_guard = token.enter();

    {
        let MAX_PACKET_SIZE: usize = 4 * 1024 * 1024;
        let b = Bump::with_capacity(MAX_PACKET_SIZE);
        b.set_allocation_limit(Some(0));
        let deserialized: (Supercow<Block>, usize) =
            Serializable::deserialize(&b, &raw_block).unwrap();
        println!("{}", deserialized.1);
        drop(deserialized);
    }

    drop(alloc_guard);
    // TODO: check if number of allocations = number of deallocations automatically,
    // for now I just inspected stdout
    AllocationRegistry::disable_tracking();
}
