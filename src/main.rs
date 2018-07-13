#![feature(plugin)]
#![plugin(rocket_codegen)]

extern crate rocket;
use rocket::State;
extern crate rocket_contrib;
use rocket_contrib::Json;

extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use rocket::response::NamedFile;
use serde::de::IgnoredAny;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Result};
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
type DependeciesMap = HashMap<String, Vec<String>>;
type MostUsedMap = HashSet<String>;
type SearchResult = (usize, f32, HashSet<String>);

#[derive(Deserialize)]
struct DependenciesJson {
    id: String,
    key: Key,
}

#[derive(Deserialize)]
struct Key(String, IgnoredAny, IgnoredAny);

fn create_dependencies_map(dependencies_path: &Path) -> Result<DependeciesMap> {
    // Remeber, I've removed first and last line and then added a comma at the
    // end of the second-last
    let file = File::open(dependencies_path)?;

    let mut dependents: DependeciesMap = HashMap::new();

    for line_opt in BufReader::new(file).lines() {
        let mut line = line_opt?;

        line.pop(); // Remove trailing comma

        let v: DependenciesJson = serde_json::from_str(&line)?;

        let id = v.key.0;
        let dependent = v.id;

        dependents.entry(id).or_insert(vec![]).push(dependent);
    }

    Ok(dependents)
}

fn create_most_used_map(most_used_path: &Path) -> Result<MostUsedMap> {
    let mut most_important_packages: HashSet<String> = HashSet::new();
    let most_important_list = File::open(most_used_path)?;
    for line in BufReader::new(most_important_list).lines() {
        most_important_packages.insert(line?);
    }
    Ok(most_important_packages)
}

fn count_depended_upon(
    dependencies_map: &DependeciesMap,
    most_used_packages: &MostUsedMap,
    package: &str,
) -> Result<SearchResult> {
    let mut packages_that_will_fail: HashSet<String> = HashSet::new();

    let mut already_downloaded: HashSet<String> = HashSet::new();

    let mut package_name: String = package.to_string();
    let mut total = 0;

    // A stack to unroll the recursion of the depth-first-search
    let mut s: Vec<String> = vec![];

    s.push(package_name);

    while !s.is_empty() {
        package_name = s.pop().unwrap();
        if most_used_packages.contains(&package_name) {
            packages_that_will_fail.insert(package_name.clone());
        }

        if already_downloaded.contains(&package_name) {
            continue;
        } else {
            already_downloaded.insert(package_name.clone());
        }

        if dependencies_map.contains_key(&package_name) {
            let deps = &dependencies_map[&package_name];

            // If a package has no deps but it's listed in file.json
            if deps.is_empty() {
                total += 1;
            } else {
                // If a package has deps
                for dep in deps {
                    s.push(dep.to_owned());
                }
            }
        } else {
            // If a package is not listed in file.json (because it has no deps, hopefully)
            total += 1;
        }
    }

    // Obtained from `https://skimdb.npmjs.com/registry/_design/app/_view/browseAll`
    let total_npm_packages = 633947;

    Ok((
        total,
        total as f32 * 100. / total_npm_packages as f32,
        packages_that_will_fail,
    ))
}

#[get("/")]
fn index() -> Result<NamedFile> {
    NamedFile::open(Path::new("static/index.html"))
}

#[get("/api/<package..>")]
fn package(
    dependencies_map: State<DependeciesMap>,
    most_used_packages: State<MostUsedMap>,
    searches_cache: State<RwLock<HashMap<String, SearchResult>>>,
    package: PathBuf,
) -> Json<SearchResult> {
    let package = package.into_os_string().into_string().unwrap();
    let mut cache = searches_cache.write().unwrap();

    let result = cache.deref_mut().entry(package.clone()).or_insert_with(|| {
        count_depended_upon(&dependencies_map, &most_used_packages, &package).unwrap()
    });

    Json(result.clone())
}

fn main() {
    let dependencies_path = Path::new("deps.json");
    let dependencies_map =
        create_dependencies_map(dependencies_path).expect("Could not create the dependencies map");

    let most_used_path = Path::new("100-most-depended-npm-packages");
    let most_used_map =
        create_most_used_map(most_used_path).expect("Could not open the most used packages list");

    let searches_cache: RwLock<HashMap<String, SearchResult>> = RwLock::new(HashMap::new());

    rocket::ignite()
        .manage(dependencies_map)
        .manage(most_used_map)
        .manage(searches_cache)
        .mount("/", routes![index, package])
        .launch();
}
