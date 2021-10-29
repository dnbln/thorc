use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{find_result::FindResult, template::Template};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TemplateIndex {
    #[serde(default)]
    pub for_remote: bool,
    #[serde(default, rename = "template")]
    pub templates: BTreeSet<Template>,
}

impl TemplateIndex {
    pub fn find<'a>(&'a self, term: &str) -> FindResult<'a> {
        let (name_and_description, (name_only, description_only)): (Vec<_>, (Vec<_>, Vec<_>)) =
            self.templates
                .iter()
                .map(|t| {
                    let n = t.name().contains(term);
                    let desc = t.description().map_or(false, |d| d.contains(term));
                    if n && desc {
                        (Some(t), (None, None))
                    } else if n {
                        (None, (Some(t), None))
                    } else if desc {
                        (None, (None, Some(t)))
                    } else {
                        (None, (None, None))
                    }
                })
                .unzip();

        fn idnt<T>(v: T) -> T {
            v
        }

        FindResult {
            name_and_description: name_and_description.into_iter().filter_map(idnt).collect(),
            name_only: name_only.into_iter().filter_map(idnt).collect(),
            description_only: description_only.into_iter().filter_map(idnt).collect(),
        }
    }

    pub fn find_exact<'a>(&'a self, name: &str) -> Option<&'a Template> {
        self.templates.iter().find(|it| it.name() == name)
    }
}
