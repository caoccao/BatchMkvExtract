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

//! The Add Files / Add Folder dialogs of the Welcome page. GPUI's picker has
//! no extension filter, so the chosen paths go through the same `.mkv`
//! filter as a drop.

use gpui_kit::{App, PathPromptOptions};

use crate::state::{Stores, files};

/// Lets the user pick `.mkv` files and adds them.
pub fn add_files(stores: &Stores, cx: &mut App) {
  prompt(
    stores,
    PathPromptOptions {
      files: true,
      directories: false,
      multiple: true,
      prompt: None,
    },
    cx,
  );
}

/// Lets the user pick a folder and adds the `.mkv` files directly inside it.
pub fn add_folder(stores: &Stores, cx: &mut App) {
  prompt(
    stores,
    PathPromptOptions {
      files: false,
      directories: true,
      multiple: false,
      prompt: None,
    },
    cx,
  );
}

fn prompt(stores: &Stores, options: PathPromptOptions, cx: &mut App) {
  let chosen = cx.prompt_for_paths(options);
  let (file_store, ui) = (stores.files.clone(), stores.ui.clone());
  cx.spawn(async move |cx| {
    let paths = match chosen.await {
      Ok(Ok(Some(paths))) => paths,
      Ok(Ok(None)) | Err(_) => return,
      Ok(Err(error)) => {
        log::error!("The file dialog failed: {error}");
        return;
      }
    };
    let paths = paths
      .into_iter()
      .map(|path| path.to_string_lossy().into_owned())
      .collect();
    cx.update(|cx| files::add_paths(&file_store, Some(&ui), paths, cx));
  })
  .detach();
}
