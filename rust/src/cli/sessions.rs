use super::*;

pub fn run(window_min: i64) -> Result<()> {
    println!("{}", session_util::format_active(window_min, 20));
    Ok(())
}
