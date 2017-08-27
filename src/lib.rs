//! A crate that implements [editorconfig](http://editorconfig.org/).
extern crate regex;
extern crate ordermap;
mod ini;
use ini::Ini;
use regex::{Regex, Captures};
use ordermap::OrderMap;

use std::path::{Path, PathBuf};
use std::fs::read_dir;
use std::ffi::OsString;
use std::io;
use std::error::Error;
fn editorconfig_is_root(file_path: &Path) -> Result<bool, ini::Error> {
    let cfg = Ini::load_from_file(file_path)?;
    if let Some(root) = cfg.get_from::<String>(None, "root") {
        if root == "true" {
            return Ok(true);
        }
    }
    Ok(false)
}

fn crawl_paths(path: &Path, conffile: &str) -> Vec<PathBuf> {
    let mut path = path.to_path_buf();
    let mut result = vec![];
    while path.parent().is_some() {
        let mut adjacent_file = path.clone();
        adjacent_file.set_file_name(conffile);
        result.push(adjacent_file);
        path.pop();
    }
    return result;
}

fn search_dir_for_editorconfig(search_path: &Path) -> io::Result<PathBuf> {
    // If we are given a relative refernece we need to make it absolute to be able to traverse until root
    let absolute_path = if search_path.is_relative() {
        search_path.canonicalize()?
    } else {
        search_path.to_path_buf()
    };

    let file_path = if absolute_path.is_dir() {
        &absolute_path
    } else {
        absolute_path.parent().unwrap()
    };
    let files = read_dir(file_path)?;

    // Search in the current directory
    for file in files {
        let path = file?.path();

        if let Some(file_name) = path.file_name() {
            if file_name == OsString::from(".editorconfig") {
                return match editorconfig_is_root(path.as_path()) {
                    Ok(true) => Ok(path.clone()),
                    Ok(false) => continue,
                    Err(e) => Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
                };
            }
        } else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Path terminates in ..",
            ));
        }
    }

    // Searches for the parent if we are not already root
    if search_path == Path::new("/") {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            ".editorconfig not found, root reached",
        ));
    } else {
        // TODO: maybe remove recursion, but it should be TCO'd
        search_dir_for_editorconfig(search_path.parent().unwrap())
    }
}

fn glob_match(pattern: &String, candidate: &String) -> bool {
    // Step 1. Escape the crap out of the existing pattern
    // println!("B: {}", pattern);
    let pattern = pattern.replace(".", r"\.");
    let unmatched_open_bracket_regex = Regex::new(r"\[([^\]]*)$").unwrap();
    let pattern = unmatched_open_bracket_regex.replace(&pattern, r"\[$1").to_string();
    // Step 2. Convert sh globs to regexes
    // Handling * and ** is weird but this actually works
    let pattern = pattern.replace("*", "[^/]*");
    let pattern = pattern.replace("[^/]*[^/]*", ".*");
    let pattern = pattern.replace("?", ".");
    let pattern = pattern.replace("[!", "[^");
    let alternation_regex = Regex::new(r"\{(.*,.*)\}").unwrap();
    let pattern = alternation_regex.replace(&pattern, |caps: &Captures| {
        let padded_cases = format!(",{},", &caps[1]);
        let quantifier = if padded_cases.contains(",,") {
            "?"
        } else {
            ""
        };
        let cases = caps[1].replace(",", "|");
        format!("({}){}", cases, quantifier)
    }).to_string();
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
    let known_keys = ["indent_style", "indent_size", "tab_width", "end_of_line", "charset", "trim_trailing_whitespace", "insert_final_newline"];
    known_keys.contains(&key)
}

/// Searches for a `.editorconfig` file and returns a struct representing its content which can be iterated.
///
/// The `file_path` argument can be the path to a directory or a file.
///
/// It looks for a file named
/// `.editorconfig` in that directory (or the file's directory) and in every parent directory.
/// A search for `.editorconfig` files will stop if the root filepath is reached
/// or an `.editorconfig` file with `root=true` is found.
///
/// # Example
/// ```rust,no_run
/// use std::path::Path;
/// let res = editorconfig::get_editorconfig(Path::new("./myfile.rs")).unwrap();
/// for (sec, prop) in res.iter() {
///    println!("Section: {:?}", *sec);
///    for (k, v) in prop.iter() {
///        println!("{}:{}", *k, *v);
///    }
/// }
/// ```
///
/// # Errors
///  It returns an error:
///
/// - when the `.editorconfig` file can't be found
///
/// - when it can't parse it
///
/// - when the `file_path` is malformed
///
pub fn get_editorconfig(file_path: &Path) -> Result<ini::Ini, Box<Error>> {
    let edc_path = search_dir_for_editorconfig(file_path)?;
    let edc = ini::Ini::load_from_file(edc_path)?;
    Ok(edc)
}

/// Finds actual configuration that applies to file with given path.
///
/// Parses .editorconfig data until root is found.
pub fn get_config(file_path: &Path) -> Result<OrderMap<String, String>, Box<Error>> {
    get_config_conffile(file_path, ".editorconfig")
}

/// Finds actual configuration that applies to file with given path.
///
/// Looks for config data in given filename; in normal operation this will be ".editorconfig".
pub fn get_config_conffile(file_path: &Path, conffile: &str) -> Result<OrderMap<String, String>, Box<Error>> {
    let paths = crawl_paths(file_path, conffile);
    let mut result = OrderMap::new();
    for conf_path in paths {
        if !conf_path.exists() {
            continue;
        }
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
    fn finds_editorconfig_in_directory() {
        let search = search_dir_for_editorconfig(Path::new("./test_files/simple/"));
        assert!(search.is_ok());
        assert_eq!(
            search.unwrap(),
            Path::new("./test_files/simple/.editorconfig")
                .canonicalize()
                .unwrap()
        );
    }
    #[test]
    fn finds_editorconfig_in_file() {
        let search = search_dir_for_editorconfig(Path::new("./test_files/simple/file.txt"));
        assert!(search.is_ok());
        assert_eq!(
            search.unwrap(),
            Path::new("./test_files/simple/.editorconfig")
                .canonicalize()
                .unwrap()
        );
    }
    #[test]
    fn traverses_until_root_editorconfig_is_found() {
        let search = search_dir_for_editorconfig(
            Path::new("./test_files/non_root_editorconfig/foo/file.txt"),
        );
        assert!(search.is_ok());
        assert_eq!(
            search.unwrap(),
            Path::new("./test_files/non_root_editorconfig/.editorconfig")
                .canonicalize()
                .unwrap()
        );
    }

    #[test]
    fn stops_at_root() {
        let search = search_dir_for_editorconfig(Path::new("./"));
        assert!(search.is_err());
        assert!(search.err().unwrap().kind() == std::io::ErrorKind::NotFound);
    }

    #[test]
    fn finds_editorconfig_multiple_directories() {
        let search =
            search_dir_for_editorconfig(Path::new("./test_files/multi_level/foo/bar/file.txt"));
        assert!(search.is_ok());
        assert_eq!(
            search.unwrap(),
            Path::new("./test_files/multi_level/.editorconfig")
                .canonicalize()
                .unwrap()
        );
    }

    #[test]
    fn checks_if_editorconfig_is_root() {
        assert!(editorconfig_is_root(Path::new("./test_files/.editorconfig")).unwrap());
        assert!(!editorconfig_is_root(
            Path::new("./test_files/.editorconfig-not-root"),
        ).unwrap());
    }

    #[test]
    fn it_gets_all_properties() {
        assert!(get_editorconfig(Path::new("./test_files/file.txt")).is_ok());
    }

    #[test]
    fn get_editorconfig_for_non_existing_file() {
        assert!(get_editorconfig(Path::new("./test_files/diocano")).is_err());
    }
}
