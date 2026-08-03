use crate::error::{MdatError, Result};

pub fn parse_slice_string(input: &str, length: usize) -> Result<Vec<usize>> {
    if input.trim().eq_ignore_ascii_case("all") {
        return Ok((0..length).collect());
    }

    let mut indices = std::collections::BTreeSet::new();
    for segment in input.split(',') {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }

        if segment.contains(':') {
            let parts: Vec<&str> = segment.split(':').collect();
            if parts.len() > 3 {
                return Err(MdatError::InvalidInput(format!(
                    "invalid slice segment: {segment:?}"
                )));
            }

            let parse_part = |part: &str| -> Result<Option<isize>> {
                if part.is_empty() {
                    Ok(None)
                } else {
                    part.parse::<isize>().map(Some).map_err(|_| {
                        MdatError::InvalidInput(format!("invalid slice segment: {segment:?}"))
                    })
                }
            };

            let start = parse_part(parts.first().copied().unwrap_or(""))?.unwrap_or(0);
            let end = parts
                .get(1)
                .map(|part| parse_part(part))
                .transpose()?
                .flatten()
                .map(|value| value as isize)
                .unwrap_or(length as isize);
            let step = parts
                .get(2)
                .map(|part| parse_part(part))
                .transpose()?
                .flatten()
                .unwrap_or(1);
            if step == 0 {
                return Err(MdatError::InvalidInput(format!(
                    "slice step cannot be zero: {segment:?}"
                )));
            }

            let mut index = start;
            if step > 0 {
                while index < end {
                    push_index(&mut indices, index, length)?;
                    index += step;
                }
            } else {
                while index > end {
                    push_index(&mut indices, index, length)?;
                    index += step;
                }
            }
        } else {
            let index = segment.parse::<isize>().map_err(|_| {
                MdatError::InvalidInput(format!("invalid slice segment: {segment:?}"))
            })?;
            push_index(&mut indices, index, length)?;
        }
    }

    if indices.is_empty() {
        return Err(MdatError::InvalidInput(format!(
            "slice string {input:?} produced no indices"
        )));
    }

    Ok(indices.into_iter().collect())
}

fn push_index(
    indices: &mut std::collections::BTreeSet<usize>,
    index: isize,
    length: usize,
) -> Result<()> {
    if index < -(length as isize) || index >= length as isize {
        return Err(MdatError::InvalidInput(format!(
            "index {index} out of range for length {length}"
        )));
    }
    let normalized = index.rem_euclid(length as isize) as usize;
    indices.insert(normalized);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_slice_string;

    #[test]
    fn parse_all() {
        assert_eq!(parse_slice_string("all", 5).unwrap(), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn parse_slice_and_indices() {
        assert_eq!(parse_slice_string("0:5,10", 12).unwrap(), vec![0, 1, 2, 3, 4, 10]);
        assert_eq!(parse_slice_string("0:10:2", 10).unwrap(), vec![0, 2, 4, 6, 8]);
    }

    #[test]
    fn parse_negative_index() {
        assert_eq!(parse_slice_string("-1", 5).unwrap(), vec![4]);
    }
}
