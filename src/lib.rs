//! A crate that implements [editorconfig](http://editorconfig.org/).
extern crate ini;
use ini::Ini;

use std::path::{Path, PathBuf};
use std::fs::read_dir;
use std::ffi::OsString;
fn editorconfig_is_root(file_path: &Path) -> Result<bool, ini::ini::Error> {
    let cfg = Ini::load_from_file(file_path)?;
    if let Some(root) = cfg.get_from::<String>(None, "root") {
        if root == "true" {
            return Ok(true);
        }
    }
    Ok(false)
}

fn search_dir_for_editorconfig(search_path: &Path) -> std::io::Result<PathBuf> {
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
pub fn get_editorconfig(file_path: &Path) -> Result<ini::Ini, Box<std::error::Error>> {
    let edc_path = search_dir_for_editorconfig(file_path)?;
    let edc = ini::Ini::load_from_file(edc_path)?;
    Ok(edc)
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
