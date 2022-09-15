macro_rules! logln {
     () => ({
        _log("\n")
     });
    ($($arg:tt) *) => ({
        _log(&format_args!($($arg)*).to_string());
    })
}
