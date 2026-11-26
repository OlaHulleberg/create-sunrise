//! A separate test binary for the process-wide hook filter.

use create_engine::states::{clear_range, index, set_range};

#[test]
fn index_misses_outside_the_range_and_when_cleared() {
    set_range(100, 3);
    assert_eq!(index(99), None);
    assert_eq!(index(100), Some(0));
    assert_eq!(index(102), Some(2));
    assert_eq!(index(103), None);
    assert_eq!(index(u32::MAX), None);
    clear_range();
    assert_eq!(index(100), None);
    assert_eq!(index(0), None);
}
