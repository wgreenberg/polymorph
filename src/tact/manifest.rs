use crate::error::Error;

#[derive(Clone)]
pub struct Manifest {
    pub field_names: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Manifest {
    pub fn parse(data: &[u8]) -> Result<Self, Error> {
        let body = std::str::from_utf8(data).unwrap();
        let mut lines = body.lines();

        let header = lines.next().unwrap();
        let mut field_names = Vec::new();
        for field_def in header.split('|') {
            let (field_name, _field_type) = field_def.split_once('!').unwrap();
            field_names.push(field_name.to_string());
        }

        let mut rows = Vec::new();
        for line in lines {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            rows.push(line.split('|').map(|s| s.to_string()).collect());
        }

        Ok(Manifest {
            field_names,
            rows,
        })
    }

    pub fn get_field_index(&self, needle: &str) -> Option<usize> {
        self.field_names.iter().position(|haystack| haystack == needle)
    }

    pub fn get_field(&self, row: usize, field: &str) -> Option<&str> {
        let field_index = self.get_field_index(field)?;
        let row = self.rows.get(row)?;
        Some(row.get(field_index)?.as_str())
    }

    pub fn find_row(&self, field: &str, value: &str) -> Option<usize> {
        let field_index = self.get_field_index(field)?;
        self.rows.iter().position(|row| row[field_index] == value)
    }
}
