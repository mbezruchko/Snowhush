//! What can be tried out before its day or its release: each turned on by SNOWHUSH_<NAME>=1 as the app starts, or built
//! into a test build that needs no setting up by naming it in SNOWHUSH_PREVIEWS as the app is compiled, as in
//! SNOWHUSH_PREVIEWS="new_year midnight fight".

pub fn on(name: &str) -> bool {
    std::env::var(format!("SNOWHUSH_{}", name.to_ascii_uppercase())).is_ok_and(|value| value == "1")
        || option_env!("SNOWHUSH_PREVIEWS").is_some_and(|built| built.split_whitespace().any(|built| built == name))
}
