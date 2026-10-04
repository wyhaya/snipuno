pub(crate) fn command_line(executable: &[u16]) -> Result<Vec<u16>, String> {
    if executable.is_empty() || executable.iter().any(|value| *value == 0 || *value == 34) {
        return Err("The Snipuno installation path is invalid.".into());
    }
    let command: Vec<_> = [34]
        .into_iter()
        .chain(executable.iter().copied())
        .chain("\" --autostart".encode_utf16())
        .chain([0])
        .collect();
    if command.len() > 260 {
        return Err("The Snipuno installation path is too long for launch at login.".into());
    }
    Ok(command)
}

pub(crate) fn is_approved(value: &[u8]) -> Result<bool, String> {
    if value.len() != 12 {
        return Err("Windows returned an invalid startup approval status.".into());
    }
    match u32::from_le_bytes(value[..4].try_into().unwrap()) {
        2 | 6 => Ok(true),
        3 | 7 => Ok(false),
        _ => Err("Windows returned an unrecognized startup approval status.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_the_exact_utf16_path_and_terminates_once() {
        for path in [
            r"C:\Program Files\截图😀\Snipuno.exe"
                .encode_utf16()
                .collect::<Vec<_>>(),
            vec![67, 58, 92, 0xd800, 46, 101, 120, 101],
        ] {
            let mut expected = vec![34];
            expected.extend_from_slice(&path);
            expected.extend("\" --autostart".encode_utf16());
            expected.push(0);
            assert_eq!(command_line(&path).unwrap(), expected);
        }
    }

    #[test]
    fn command_rejects_invalid_paths_and_checks_the_complete_length() {
        for path in [vec![], vec![65, 0, 66], vec![65, 34, 66], vec![65; 246]] {
            assert!(command_line(&path).is_err());
        }
        assert_eq!(command_line(&vec![65; 245]).unwrap().len(), 260);
    }

    #[test]
    fn approval_decodes_known_states_and_rejects_malformed_records() {
        for state in [0u32, 2, 3, 6, 7, 8, 0x10000002] {
            let mut bytes = [255; 12];
            bytes[..4].copy_from_slice(&state.to_le_bytes());
            match state {
                2 | 6 => assert_eq!(is_approved(&bytes), Ok(true)),
                3 | 7 => assert_eq!(is_approved(&bytes), Ok(false)),
                _ => assert!(is_approved(&bytes).is_err()),
            }
        }
        for len in [0, 4, 11, 13] {
            assert!(is_approved(&vec![2; len]).is_err());
        }
    }
}
