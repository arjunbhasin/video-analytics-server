use std::fs;
use pyo3::prelude::*;

#[pyfunction]
pub fn get_person(filepath: &str) -> PyResult<String> {
    Python::with_gil(|py| {
        let python_code = fs::read_to_string("get_person.py").unwrap();
        let get_person_from_filepath = PyModule::from_code_bound(
            py,
            python_code.as_str(),
            "get_person.py",
            "get_person",
        )?;

        let detections: String = get_person_from_filepath
            .getattr("get_person_from_filepath")?
            .call1((filepath,))?
            .extract()?;
        Ok(detections)
    })
}