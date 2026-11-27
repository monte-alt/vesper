//! Verify materializing a replay list reuses a completed native list and
//! its private budget, while old GPU commands do not execute again.
use super::*;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::Interface;

struct RestoreIdle(Option<(Vec<Idle>, u64)>);
impl Drop for RestoreIdle {
    fn drop(&mut self) {
        let previous = self.0.take().unwrap();
        let discard = std::mem::replace(&mut *IDLE.lock().unwrap_or_else(|e| e.into_inner()), previous);
        drop(discard);
    }
}

#[test]
#[ignore = "requires a hardware GPU and an isolated process for the global idle pool"]
fn replay_factory_reuses_completed_list_and_budget_without_replaying_old_commands() {
    assert!(pools(), "this regression exercises the default enabled list pool");
    let previous = std::mem::take(&mut *IDLE.lock().unwrap_or_else(|e| e.into_inner()));
    let _restore = RestoreIdle(Some(previous));
    let host = crate::host::host().expect("a hardware D3D12 device");
    let first = host.new_list().unwrap();
    let identity = first.list.as_raw();
    let budget = first.budget().expect("the private indirect budget");
    let (a, a_data) = host.upload_buffer(64).unwrap();
    let (b, b_data) = host.upload_buffer(64).unwrap();
    let out_a = host.read_back_buffer(64).unwrap();
    let out_b = host.read_back_buffer(64).unwrap();
    // SAFETY: live, exclusively owned 64-byte upload mappings.
    unsafe {
        std::ptr::write_bytes(a_data, 0x11, 64);
        std::ptr::write_bytes(b_data, 0x22, 64);
        first.list.CopyBufferRegion(&out_a, 0, &a, 0, 64);
    }
    first.close();
    let queue: ID3D12CommandQueue = unsafe {
        host.device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
            Type: D3D12_COMMAND_LIST_TYPE_DIRECT,
            ..Default::default()
        }).unwrap()
    };
    let wait = |list: &ID3D12GraphicsCommandList| unsafe {
        queue.ExecuteCommandLists(&[Some(list.cast().unwrap())]);
        let fence: ID3D12Fence = host.device.CreateFence(0, D3D12_FENCE_FLAG_NONE).unwrap();
        queue.Signal(&fence, 1).unwrap();
        while fence.GetCompletedValue() < 1 {
            host.device.GetDeviceRemovedReason().unwrap();
            std::thread::yield_now();
        }
    };
    wait(&first.list);
    recycle(vec![Detached { host: first, _uploads: Vec::new(), _state: Default::default() }], &[0]);
    assert_eq!(IDLE.lock().unwrap_or_else(|e| e.into_inner()).0.len(), 1);
    // A stale copy command would now overwrite out_a with 0x33.
    unsafe { std::ptr::write_bytes(a_data, 0x33, 64); }
    let mut replay = boxed(0, std::ptr::null(), false);
    let second = replay.host.take().expect("materialized replay host list");
    assert_eq!(second.list.as_raw(), identity, "the replay factory bypassed the completed-list pool");
    assert_eq!(second.budget().unwrap(), budget, "the private budget must survive reuse");
    unsafe { second.list.CopyBufferRegion(&out_b, 0, &b, 0, 64); }
    second.close();
    wait(&second.list);
    for (resource, expected) in [(&out_a, 0x11), (&out_b, 0x22)] {
        let mut mapped = std::ptr::null_mut();
        unsafe {
            resource.Map(0, Some(&D3D12_RANGE { Begin: 0, End: 64 }), Some(&mut mapped)).unwrap();
            let actual = std::slice::from_raw_parts(mapped.cast::<u8>(), 64).to_vec();
            resource.Unmap(0, Some(&D3D12_RANGE { Begin: 0, End: 0 }));
            assert_eq!(actual, vec![expected; 64]);
        }
    }
    recycle(vec![Detached { host: second, _uploads: Vec::new(), _state: Default::default() }], &[0]);
}
