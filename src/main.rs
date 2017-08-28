extern crate editorconfig;
extern crate argparse;

use editorconfig::*;
use argparse::{ArgumentParser, Store, List, Print};
use std::path::Path;

fn main() {
    let mut conf_filename = ".editorconfig".to_string();
    let mut version = "".to_string();
    let mut targets: Vec<String> = vec![];
    {
        let mut ap = ArgumentParser::new();
        ap.set_description("Parse .editorconfig files.");
        ap.refer(&mut conf_filename)
            .add_option(&["-f"], Store, "Conf filename");
        ap.refer(&mut version)
            .add_option(&["-b"], Store, "editorconfig version");
        ap.add_option(&["-v", "--version"],
            Print(format!("EditorConfig Rust Core Version {}", env!("CARGO_PKG_VERSION"))), "Show version");
        ap.refer(&mut targets)
            .add_argument("arguments", List, "Files to check");
        ap.parse_args_or_exit();
    }
    
    let multiple_targets = targets.len() > 1;
    
    for t in targets {
        if multiple_targets {
            println!("[{}]", t);
        }

        let res = get_config_conffile(Path::new(&t), &conf_filename).unwrap();
        for (k, v) in res.iter() {
            println!("{}={}", *k, *v);
        }
    }
}
