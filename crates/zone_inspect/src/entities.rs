use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityCensus {
    pub entities: usize,
    pub classnames: BTreeMap<String, usize>,
    pub targetnames: BTreeMap<String, usize>,
    pub keys: BTreeMap<String, usize>,
}

pub fn census(text: &str) -> EntityCensus {
    let mut census = EntityCensus::default();
    let mut open = false;
    for line in text.lines() {
        let line = line.trim();
        match line {
            "" => {}
            "{" => {
                open = true;
                census.entities += 1;
            }
            "}" => open = false,
            _ if open => {
                let Some((key, value)) = pair(line) else {
                    continue;
                };
                *census.keys.entry(key.to_owned()).or_insert(0) += 1;
                match key {
                    "classname" => *census.classnames.entry(value.to_owned()).or_insert(0) += 1,
                    "targetname" => *census.targetnames.entry(value.to_owned()).or_insert(0) += 1,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    census
}

fn pair(line: &str) -> Option<(&str, &str)> {
    let (key, rest) = if let Some(quoted) = line.strip_prefix('"') {
        let end = quoted.find('"')?;
        (&quoted[..end], &quoted[end + 1..])
    } else {
        line.split_once(char::is_whitespace)?
    };
    let value = rest.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or(value);
    (!key.is_empty()).then_some((key, value))
}

pub fn by_count(counts: &BTreeMap<String, usize>) -> Vec<(&str, usize)> {
    let mut rows: Vec<(&str, usize)> = counts.iter().map(|(k, &n)| (k.as_str(), n)).collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTITIES: &str = "{\n\"classname\" \"worldspawn\"\n\"ambienttrack\" \"\"\n}\n{\n\"origin\" \"0 0 0\"\n\"classname\" \"info_player_start\"\n}\n{\n\"targetname\" \"zone_start\"\n\"classname\" \"info_volume\"\n}\n{\nclassname \"info_volume\"\ntargetname \"zone_start\"\n}\n";

    #[test]
    fn counts_entities_classnames_and_targetnames() {
        let census = census(ENTITIES);
        assert_eq!(census.entities, 4);
        assert_eq!(census.classnames["info_volume"], 2);
        assert_eq!(census.classnames["worldspawn"], 1);
        assert_eq!(census.targetnames["zone_start"], 2);
        assert_eq!(census.keys["classname"], 4);
        assert_eq!(census.keys["ambienttrack"], 1);
    }

    #[test]
    fn values_keep_spaces() {
        assert_eq!(pair("\"origin\" \"1 2 3\""), Some(("origin", "1 2 3")));
        assert_eq!(pair("origin \"1 2 3\""), Some(("origin", "1 2 3")));
        assert_eq!(pair("\"\" \"x\""), None);
    }

    #[test]
    fn ranks_by_count_then_name() {
        let census = census(ENTITIES);
        let rows = by_count(&census.classnames);
        assert_eq!(rows[0], ("info_volume", 2));
        assert_eq!(rows[1], ("info_player_start", 1));
    }
}
