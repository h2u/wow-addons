fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = if args.len() >= 3 {
        wow_addons_core::inventory::run(&args[1], &args[2], &args[3..])
    } else {
        Err(
            "Usage: wow-addons-core <discover|scan|inspect-zip|install-zip|remove> <path> ..."
                .into(),
        )
    };
    match result {
        Ok(data) => println!("{}", serde_json::json!({"ok":true,"data":data})),
        Err(error) => {
            println!("{}", serde_json::json!({"ok":false,"error":error}));
            std::process::exit(1);
        }
    }
}
