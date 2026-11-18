/*
*   Copyright (c) 2026. caoccao.com Sam Cao
*   All rights reserved.

*   Licensed under the Apache License, Version 2.0 (the "License");
*   you may not use this file except in compliance with the License.
*   You may obtain a copy of the License at

*   http://www.apache.org/licenses/LICENSE-2.0

*   Unless required by applicable law or agreed to in writing, software
*   distributed under the License is distributed on an "AS IS" BASIS,
*   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*   See the License for the specific language governing permissions and
*   limitations under the License.
*/

//! New files of this process's own, for writing a file under a name that
//! no one else uses before it takes its real one.

use std::fs::File;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// How many names [`create`] tries before it gives up.
const ATTEMPTS: u32 = 100;

/// Creates a file that no one else has opened at `path_for(tag)`, where
/// `tag` (`<process id>-<counter>`) is new for every call in this process. A
/// name left behind by an earlier process with the same id is skipped.
pub(crate) fn create(path_for: impl Fn(&str) -> PathBuf) -> std::io::Result<(PathBuf, File)> {
  static NEXT: AtomicU64 = AtomicU64::new(0);
  let mut attempt = 0;
  loop {
    let tag = format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let path = path_for(&tag);
    match File::options().write(true).create_new(true).open(&path) {
      Ok(file) => return Ok((path, file)),
      Err(error) if error.kind() == ErrorKind::AlreadyExists && attempt < ATTEMPTS => attempt += 1,
      Err(error) => return Err(error),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_call_creates_a_new_file_and_skips_names_that_are_taken() {
    let dir = std::env::temp_dir().join(format!("bme_unique_file_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (first, _) = create(|tag| dir.join(format!("a.{tag}.tmp"))).unwrap();
    let (second, _) = create(|tag| dir.join(format!("a.{tag}.tmp"))).unwrap();
    assert_ne!(first, second);
    let error = create(|_| first.clone()).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::AlreadyExists, "one name only: it gives up");
    let _ = std::fs::remove_dir_all(&dir);
  }
}
