/// Builds the source-aware three-way diff shown by the management portal.
/// A field is returned only when the local working record or incoming Feishu
/// snapshot differs from the last accepted source snapshot.
pub fn three_way_diff(
    last_accepted_base: &Value,
    current_local: &Value,
    incoming_feishu: &Value,
    source_owned_paths: &BTreeSet<String>,
) -> Vec<FieldDiff> {
    let base = flatten(last_accepted_base);
    let local = flatten(current_local);
    let incoming = flatten(incoming_feishu);
    let paths: BTreeSet<_> = base
        .keys()
        .chain(local.keys())
        .chain(incoming.keys())
        .cloned()
        .collect();

    paths
        .into_iter()
        .filter_map(|path| {
            let base_value = base.get(&path).cloned();
            let local_value = local.get(&path).cloned();
            let incoming_value = incoming.get(&path).cloned();
            if base_value == local_value && base_value == incoming_value {
                return None;
            }
            Some(FieldDiff {
                field_path: path.clone(),
                base_value,
                local_value,
                incoming_value,
                source_owned: source_owned_paths.contains(&path),
            })
        })
        .collect()
}

pub fn conflicting_diffs(diffs: &[FieldDiff]) -> Vec<FieldDiff> {
    diffs
        .iter()
        .filter(|diff| {
            diff.source_owned
                && diff.local_value != diff.base_value
                && diff.incoming_value != diff.base_value
                && diff.local_value != diff.incoming_value
        })
        .cloned()
        .collect()
}
