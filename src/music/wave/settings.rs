// SPDX-License-Identifier: GPL-3.0-or-later
//! Service-advertised enum settings for a new personalized Wave session.

use super::{MusicError, MusicService, WaveSource};
use crate::session::AccessToken;
use serde::Deserialize;

const MAX_OPTIONS: usize = 24;

#[derive(Clone, Copy)]
enum Category {
    Diversity,
    Mood,
    Language,
}

impl Category {
    const ALL: [Self; 3] = [Self::Diversity, Self::Mood, Self::Language];

    fn kind(self) -> &'static str {
        match self {
            Self::Diversity => "settingDiversity",
            Self::Mood => "settingMoodEnergy",
            Self::Language => "settingLanguage",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Diversity => "Характер",
            Self::Mood => "Настроение",
            Self::Language => "Язык",
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct WaveTuning {
    tags: [Option<String>; Category::ALL.len()],
}

impl WaveTuning {
    pub(crate) fn selection(&self, group: &WaveSettingGroup) -> u32 {
        self.tags[group.category as usize]
            .as_ref()
            .and_then(|tag| group.choices.iter().position(|option| option.tag == *tag))
            .map_or(0, |index| index as u32 + 1)
    }

    pub(crate) fn choose(&mut self, group: &WaveSettingGroup, index: usize) {
        self.tags[group.category as usize] = index
            .checked_sub(1)
            .and_then(|index| group.choices.get(index))
            .map(|option| option.tag.clone());
    }

    pub(crate) fn into_source(self) -> WaveSource {
        if self.tags.iter().all(Option::is_none) {
            WaveSource::Personal
        } else {
            WaveSource::TunedPersonal(self)
        }
    }

    pub(super) fn seed_pairs(&self) -> impl Iterator<Item = (&'static str, &str)> {
        Category::ALL
            .into_iter()
            .zip(&self.tags)
            .filter_map(|(category, tag)| Some((category.kind(), tag.as_deref()?)))
    }
}

pub(crate) struct WaveSettingGroup {
    category: Category,
    default_label: String,
    choices: Vec<Choice>,
}

impl WaveSettingGroup {
    pub(crate) fn label(&self) -> &'static str {
        self.category.label()
    }

    pub(crate) fn labels(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.default_label.as_str())
            .chain(self.choices.iter().map(|choice| choice.label.as_str()))
    }
}

struct Choice {
    label: String,
    tag: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsWire {
    setting_restrictions: Option<RestrictionsWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestrictionsWire {
    diversity: Option<GroupWire>,
    mood_energy: Option<GroupWire>,
    language: Option<GroupWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GroupWire {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    possible_values: Vec<ChoiceWire>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChoiceWire {
    name: Option<String>,
    value: Option<serde_json::Value>,
    serialized_seed: Option<String>,
    #[serde(default)]
    unspecified: bool,
}

impl GroupWire {
    fn normalize(self, category: Category) -> Option<WaveSettingGroup> {
        if self.kind.as_deref() != Some("enum") {
            return None;
        }
        let mut default_label = None;
        let mut choices: Vec<Choice> = Vec::new();
        for value in self.possible_values.into_iter().take(MAX_OPTIONS) {
            let Some(label) = value.name.as_deref().map(str::trim).filter(|label| {
                !label.is_empty() && label.len() <= 120 && !label.chars().any(char::is_control)
            }) else {
                continue;
            };
            let Some((kind, tag)) = value
                .serialized_seed
                .as_deref()
                .and_then(|seed| seed.split_once(':'))
            else {
                continue;
            };
            if kind != category.kind()
                || value.value.as_ref().and_then(serde_json::Value::as_str) != Some(tag)
                || tag.is_empty()
                || tag.len() > 48
                || !tag
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            {
                continue;
            }
            if value.unspecified {
                default_label.get_or_insert_with(|| label.to_owned());
            } else if !choices.iter().any(|choice| choice.tag == tag) {
                choices.push(Choice {
                    label: label.to_owned(),
                    tag: tag.to_owned(),
                });
            }
        }
        (!choices.is_empty()).then(|| WaveSettingGroup {
            category,
            default_label: default_label.unwrap_or_else(|| "Без настройки".into()),
            choices,
        })
    }
}

impl MusicService {
    pub(crate) fn personal_wave_settings(
        &self,
        token: &AccessToken,
    ) -> Result<Vec<WaveSettingGroup>, MusicError> {
        let binding = self.search_binding(token)?;
        let response: SettingsWire = self.get(
            token,
            &["rotor", "wave", "settings"],
            &[("seeds", "user:onyourwave")],
        )?;
        self.check_search_binding(token, &binding)?;
        let Some(groups) = response.setting_restrictions else {
            return Ok(Vec::new());
        };
        Ok(Category::ALL
            .into_iter()
            .zip([groups.diversity, groups.mood_energy, groups.language])
            .filter_map(|(category, group)| group?.normalize(category))
            .collect())
    }
}
