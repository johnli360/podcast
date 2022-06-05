macro_rules! print_raw {
    ($($arg:tt) *) => ({
        if let Err(err) = crossterm::terminal::disable_raw_mode() {
            eprintln!("{err}");
        };
        print!("{}", format_args!($($arg)*));
        if let Err(err) = crossterm::terminal::enable_raw_mode() {
            eprintln!("{err}");
        };
    })
}

macro_rules! println_raw {
     () => ({
        if let Err(err) = crossterm::terminal::disable_raw_mode() {
            eprintln!("{err}");
        };
        print!("\n")
        if let Err(err) = crossterm::terminal::enable_raw_mode() {
            eprintln!("{err}");
        };

     });
    ($($arg:tt) *) => ({
        if let Err(err) = crossterm::terminal::disable_raw_mode() {
            eprintln!("{err}");
        };
        println!("{}", format_args!($($arg)*));
        if let Err(err) = crossterm::terminal::enable_raw_mode() {
            eprintln!("{err}");
        };
    })
}

macro_rules! eprintln_raw {
     () => ({
        if let Err(err) = crossterm::terminal::disable_raw_mode() {
            eprintln!("{err}");
        };
        eprint!("\n")
        if let Err(err) = crossterm::terminal::enable_raw_mode() {
            eprintln!("{err}");
        };

     });
    ($($arg:tt) *) => ({
        if let Err(err) = crossterm::terminal::disable_raw_mode() {
            eprintln!("{err}");
        };
        eprintln!("{}", format_args!($($arg)*));
        if let Err(err) = crossterm::terminal::enable_raw_mode() {
            eprintln!("{err}");
        };
    })
}
