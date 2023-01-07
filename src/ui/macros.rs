#[macro_export]
macro_rules! logln {
     () => ({
        $crate::ui::log::_log("\n")
     });
    ($($arg:tt) *) => ({
        $crate::ui::log::_log(format_args!($($arg)*));
    })
}

