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

//! The File List and Queue tabs, plus the closable About tab. The File List
//! shows the Welcome page while it has no files; the Queue tab stays empty
//! until its view exists.

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::tab::{Tab, TabBar, TabVariant};
use gpui_kit::component::{ActiveTheme, Sizable, v_flex};
use gpui_kit::*;

use super::about::About;
use super::file_list::FileList;
use crate::state::{Stores, TabType};

pub struct MainContent {
  stores: Stores,
  file_list: Entity<FileList>,
  _subscriptions: Vec<Subscription>,
}

impl MainContent {
  pub fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
    let subscriptions = vec![cx.observe(&stores.ui, |_, _, cx| cx.notify())];
    Self {
      file_list: cx.new(|cx| FileList::new(stores.clone(), cx)),
      stores,
      _subscriptions: subscriptions,
    }
  }

  fn tab(&self, tab: TabType, cx: &mut Context<Self>) -> Tab {
    let label = Tab::new().label(tr!(tab.label_key()));
    if !tab.is_closable() {
      return label;
    }
    label.suffix(
      Button::new(SharedString::from(format!("tab-close-{}", tab.name())))
        .ghost()
        .xsmall()
        .icon(IconName::X)
        .accessibility_label(tr!("tabs.close"))
        .tooltip(tr!("tabs.close"))
        .on_click(cx.listener(move |this, _, _, cx| {
          // The click closes the tab without selecting it first.
          cx.stop_propagation();
          this.stores.ui.update(cx, |ui, cx| ui.close_tab(tab, cx));
        })),
    )
  }
}

impl Render for MainContent {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let ui = self.stores.ui.read(cx);
    let (tabs, active, index) = (ui.tabs(), ui.active_tab(), ui.active_index());
    let tab_elements: Vec<Tab> = tabs.iter().map(|tab| self.tab(*tab, cx)).collect();
    let content = match active {
      TabType::FileList => self.file_list.clone().into_any_element(),
      TabType::Queue => div().id("queue-empty").test_support().into_any_element(),
      TabType::About => About.into_any_element(),
    };
    let theme = cx.theme();
    v_flex()
      .id("main-content")
      .test_support()
      .size_full()
      .overflow_hidden()
      .child(
        TabBar::new("main-tabs")
          .with_variant(TabVariant::Tab)
          .mt(super::SHELL_MARGIN)
          .bg(theme.tokens.muted)
          .border_t_1()
          .border_color(theme.border)
          .selected_index(index)
          .children(tab_elements)
          .on_click(cx.listener(move |this, index: &usize, _, cx| {
            if let Some(tab) = tabs.get(*index).copied() {
              this.stores.ui.update(cx, |ui, cx| ui.set_active_tab(tab, cx));
            }
          })),
      )
      .child(
        v_flex()
          .id(SharedString::from(format!("panel-{}", active.name())))
          .flex_1()
          .min_h_0()
          .w_full()
          .py(super::SHELL_MARGIN)
          .overflow_y_scroll()
          .child(content),
      )
  }
}
