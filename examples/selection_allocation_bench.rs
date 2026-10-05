use be6500_panel::{
    native::{CompileInput, compile_native},
    rule_apply::{compile_selection, selection_settings},
};
use serde_json::Value;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};
struct Count;
static ENABLED: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() && ENABLED.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, layout, size) };
        if !next.is_null() && ENABLED.load(Ordering::Relaxed) {
            CALLS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size as u64, Ordering::Relaxed);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;
fn measure<T>(run: impl FnOnce() -> T) -> (T, u64, u64) {
    CALLS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
    ENABLED.store(true, Ordering::Relaxed);
    let output = run();
    ENABLED.store(false, Ordering::Relaxed);
    (
        output,
        CALLS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
    )
}
fn main() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/native.json")).unwrap();
    let input: CompileInput = serde_json::from_value(fixture["cases"][2]["input"].clone()).unwrap();
    let accepted = compile_native(&input).unwrap();
    let prepared = selection_settings(&accepted.config, &input.node).unwrap();
    let (discarded, calls, bytes) =
        measure(|| selection_settings(&accepted.config, &input.node).unwrap());
    drop(discarded);
    let (output, compile_calls, compile_bytes) =
        measure(|| compile_selection(Some(&accepted.config), prepared).unwrap());
    let fresh = selection_settings(&accepted.config, &input.node).unwrap();
    let again = compile_selection(Some(&accepted.config), fresh).unwrap();
    assert_eq!(again.config, output.config);
    assert_eq!(again.sha256, output.sha256);
    println!(
        "{}",
        serde_json::json!({"source":"host diagnostic, synthetic fixed fixture","removedOperation":"one duplicate owned selection_settings derivation","removedAllocationCalls":calls,"removedCumulativeAllocatedBytes":bytes,"optimizedCompileAllocationCalls":compile_calls,"optimizedCompileCumulativeAllocatedBytes":compile_bytes,"configBytes":output.config.len(),"sha256":output.sha256,"exactRepeatOutput":true,"targetRSSMeasured":false,"aggregatePerformanceGainMeasured":false})
    );
}
