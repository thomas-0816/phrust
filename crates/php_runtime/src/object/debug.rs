use super::ClassPropertyEntry;

pub(super) fn property_debug_label(property: &ClassPropertyEntry, display_class: &str) -> String {
    if property.flags.is_private {
        let name = property
            .name
            .strip_prefix("private:")
            .and_then(|rest| rest.split_once(':'))
            .map(|(_, name)| name)
            .unwrap_or(property.name.as_str());
        format!("\"{name}\":\"{display_class}\":private")
    } else if property.flags.is_protected {
        format!("\"{}\":protected", property.name)
    } else {
        format!("\"{}\"", property.name)
    }
}
