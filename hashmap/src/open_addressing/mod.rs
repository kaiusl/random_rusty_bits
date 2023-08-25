pub mod linear_probing;
pub mod quadratic_probing;
pub mod robin_hood;

#[cfg(test)]
mod metrics;

fn round_up_to_power_of_two(v: usize) -> usize {
    if v.is_power_of_two() {
        v
    } else {
        2usize.pow(v.ilog2() + 1)
    }
}
