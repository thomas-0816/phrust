use super::ClassPropertyEntry;

pub(super) fn property_debug_label(property: &ClassPropertyEntry, display_class: &str) -> String {
    if property.flags.is_private {
        if let Some((owner, name)) = property
            .name
            .strip_prefix("private:")
            .and_then(|rest| rest.split_once(':'))
        {
            format!("\"{name}\":\"{owner}\":private")
        } else {
            format!("\"{}\":\"{display_class}\":private", property.name)
        }
    } else if property.flags.is_protected {
        format!("\"{}\":protected", property.name)
    } else {
        format!("\"{}\"", property.name)
    }
}
