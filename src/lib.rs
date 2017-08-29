//! A crate that implements [editorconfig](http://editorconfig.org/).
extern crate regex;

extern crate ordermap;

mod ini;
use regex::{Regex, Captures};

use ordermap::OrderMap;

use std::path::{Path, PathBuf};
use std::error::Error;

/// Finds all possible `conffile`s starting from `path` until root.
fn crawl_paths(path: &Path, conffile: &str) -> Result<Vec<PathBuf>, Box<Error>> {
    let mut path = path.canonicalize()?;
    let mut result = vec![];
    while path.parent().is_some() {
        let mut adjacent_file = path.clone();
        adjacent_file.set_file_name(conffile);
        path.pop();
        if !adjacent_file.exists() {
            continue;
        }
        result.push(adjacent_file);
    }
    return Ok(result);
}

fn glob_match(pattern: &String, candidate: &String) -> bool {
    // Step 1. Escape the crap out of the existing pattern
    // println!("B: {}", pattern);
    let pattern = pattern.replace(".", r"\.");
    let unmatched_open_bracket_regex = Regex::new(r"\[([^\]]*)$").unwrap();
    let pattern = unmatched_open_bracket_regex.replace(&pattern, r"\[$1")
        .to_string();
    // Step 2. Convert sh globs to regexes
    // Handling * and ** is weird but this actually works
    let pattern = pattern.replace("*", "[^/]*");
    let pattern = pattern.replace("[^/]*[^/]*", ".*");
    let pattern = pattern.replace("?", ".");
    let pattern = pattern.replace("[!", "[^");
    let alternation_regex = Regex::new(r"\{(.*,.*)\}").unwrap();
    let pattern = alternation_regex.replace(&pattern, |caps: &Captures| {
            let padded_cases = format!(",{},", &caps[1]);
            let quantifier = if padded_cases.contains(",,") { "?" } else { "" };
            let cases = caps[1].replace(",", "|");
            format!("({}){}", cases, quantifier)
        })
        .to_string();
    let pattern = pattern.replace("{", r"\{");
    let pattern = pattern.replace("}", r"\}");
    let pattern = pattern.replace("||", "|");
    let pattern = pattern.replace("(|", "(");
    let pattern = pattern.replace("|)", ")");
    let mut pattern = pattern;
    pattern.push('$');
    // println!("A: {}", pattern);
    // Step 3. Actually do the testing
    let final_regex = Regex::new(&pattern).unwrap();
    return final_regex.is_match(candidate);
}

fn parse_config(target: &Path, conf_file: &Path) -> Result<OrderMap<String, String>, Box<Error>> {
    let ini_data = ini::Ini::load_from_file(conf_file)?;
    let mut result = OrderMap::new();
    if let Some(general) = ini_data.section::<String>(None) {
        if let Some(root) = general.get("root") {
            if root.to_lowercase() == "true" {
                result.insert("root".to_string(), "true".to_string());
            }
        }
    }
    let target = target.as_os_str().to_os_string().into_string().unwrap();
    #[cfg(windows)]
    let target = target.replace("\\", "/");
    for (label, data) in ini_data.iter() {
        if let Some(ref label) = *label {
            if label.len() > 4096 {
                continue;
            }
            if glob_match(label, &target) {
                for (k, v) in data.iter() {
                    result.insert(k.clone(), v.clone());
                }
            }
        }
    }

    // Preprocessing may or may not actually be part of the spec
    // so I'm stealing this from editorconfig-core-py
    if let Some(indent_style) = result.clone().get("indent_style") {
        if indent_style == "tab" {
            if result.get("indent_size").is_none() {
                result.insert("indent_size".to_string(), "tab".to_string());
            }
        }
    }
    if let Some(indent_size) = result.clone().get("indent_size") {
        if indent_size != "tab" {
            if result.get("tab_width").is_none() {
                result.insert("tab_width".to_string(), indent_size.clone());
            }
        } else {
            if let Some(tab_width) = result.clone().get("tab_width") {
                result.insert("indent_size".to_string(), tab_width.clone());
            }
        }
    }
    Ok(result)
}

fn is_known_key(key: &str) -> bool {
    let known_keys = ["indent_style",
                      "indent_size",
                      "tab_width",
                      "end_of_line",
                      "charset",
                      "trim_trailing_whitespace",
                      "insert_final_newline"];
    known_keys.contains(&key)
}



/// Finds the configuration that applies to the file passed in `file_path`.
///
/// The `file_path` argument is the path to a file.
///
/// It looks for a file named `.editorconfig` in the file's directory and in every parent directory.
/// A search for `.editorconfig` files will stop if root (`/`) is reached or an `.editorconfig` file with `root=true` is found.
///
/// # Example
/// ```
/// use std::path::Path;
///
/// let res = editorconfig::get_config(Path::new("./test_files/simple/file.txt")).unwrap();
/// for (k, v) in res.iter() {
///     println!("{}={}", *k, *v);
/// }
/// ```
/// # Errors
///  It returns an error:
///
/// - when the `.editorconfig` file can't be found
///
/// - when it can't parse it
///
/// - when the `file_path` is malformed (check `std::fs::canonicalize` docs) or is a directory.
///
pub fn get_config(file_path: &Path) -> Result<OrderMap<String, String>, Box<Error>> {
    get_config_conffile(file_path, ".editorconfig")
}

/// Finds actual configuration that applies to file with given path.
/// # MAINLY USED FOR TESTING AND INTERNAL USE, CHECK `get_config`.
///
/// Looks for config data in given filename; in normal operation this will be ".editorconfig".
pub fn get_config_conffile(file_path: &Path,
                           conffile: &str)
                           -> Result<OrderMap<String, String>, Box<Error>> {
    let paths = crawl_paths(file_path, conffile)?;
    if paths.is_empty() {
        return Err(Box::new(std::io::Error::from(std::io::ErrorKind::NotFound)));
    }
    let mut result = OrderMap::new();
    for conf_path in paths {
        let options = parse_config(file_path, &conf_path)?;
        for (k, v) in options.iter() {
            let k = k.to_lowercase();
            let v = if is_known_key(&k) {
                v.to_lowercase()
            } else {
                v.clone()
            };
            if k.len() > 50 || v.len() > 255 {
                continue;
            }
            if !result.contains_key(&k) && k != "root" {
                result.insert(k, v);
            }
        }
        if let Some(root) = options.get("root") {
            if root.to_lowercase() == "true" {
                break;
            }
        }
    }
    return Ok(result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn works_with_multi_level_directories() {
        let cfg = get_config(Path::new("./test_files/multi_level/foo/bar/file.txt")).unwrap();
        let mut map = OrderMap::new();
        map.insert("end_of_line".to_owned(), "lf".to_owned());
        map.insert("insert_final_newline".to_owned(), "true".to_owned());
        assert_eq!(cfg, map);
    }

    #[test]
    #[should_panic]
    fn get_editorconfig_for_non_existing_file() {
        assert!(get_config(Path::new("./test_files/diocano")).is_ok());
    }
}
