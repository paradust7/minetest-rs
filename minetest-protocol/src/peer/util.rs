// Minetest uses 16-bit sequence numbers that wrap around.
// To simplify reasoning about sequence numbers, translate
// them into 64-bit unique ids.
pub(crate) fn rel_to_abs(base: u64, seqnum: u16) -> u64 {
    let delta = relative_distance(base as u16, seqnum);
    ((base as i64) + delta) as u64
}

/// Determine the distance from sequence number a to b.
/// Sequence numbers are modulo 65536, so this is the
/// unique value d in the range -32768 < d <= 32768
/// with: a + d = b (mod 65536)
pub(crate) fn relative_distance(a: u16, b: u16) -> i64 {
    let d: u16 = (std::num::Wrapping(b) - std::num::Wrapping(a)).0;
    if d <= 32768 {
        d as i64
    } else {
        (d as i64) - 65536
    }
}
