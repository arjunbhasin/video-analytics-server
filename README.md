# Rust Actix-Web Server with Python Integration

This project is a Rust-based web server built using Actix-Web, with Python integration via the `pyo3` crate. It serves HTML templates and allows for interaction between Rust and Python for processing video detections.

## Features

- Serves HTML templates using Askama.
- Integrates with a SQLite database using SQLx.
- Calls Python functions from Rust using `pyo3`.
- Handles video detections and extracts bounding boxes as base64 encoded images.

## Project Structure

```
.
├── Cargo.toml
├── build.rs
├── src
│   ├── main.rs
│   └── templates
│       ├── index.html
│       └── hour.html
└── processing_results.db
```

## Prerequisites

- Rust and Cargo: [Install Rust](https://www.rust-lang.org/tools/install)
- Python 3.x: [Install Python](https://www.python.org/downloads/)


## Usage

1. Start the server:

   ```sh
   cargo run
   ```

2. Open your web browser and navigate to `http://127.0.0.1:8080` to view the index page.

3. The index page lists video records with detections. Click on the hour link to view details and extracted images for that specific hour.

## Python Integration

This project uses the `pyo3` crate to call Python functions from Rust. Specifically, it uses a Python function to extract bounding boxes from video frames and return them as base64 encoded images.

### How Python is Used Inside Rust

1. **Python Function**: The Python function `extract_box_as_b64` is defined in a Python module named `extract_box`. This function takes a video file path and detection data, processes the video, and returns a base64 encoded image.

2. **Rust Integration**: The Rust code uses `pyo3` to call this Python function. The `extract_box_as_b64` function in Rust acquires the GIL (Global Interpreter Lock) and calls the corresponding Python function.

3. **Example Usage in Rust**:

   ```rust
   #[pyfunction]
   fn extract_box_as_b64(filepath: &str, detection: Detection) -> PyResult<String> {
       Python::with_gil(|py| {
           let extract_boxes_as_b64 = PyModule::import(py, "extract_box")?;
           let image = extract_boxes_as_b64
               .getattr("extract_box_as_b64")?
               .call1((filepath, detection))?
               .extract()?;
           Ok(image)
       })
   }
   ```

## Example `extract_box.py` Python Module

Ensure you have the following Python script in the project root or in a Python module that your Rust code can reference:

```python
import cv2
from PIL import Image
import io
import base64
from typing import List, Dict

def extract_box_as_b64(filepath: str, detection: Dict) -> str:
    cap = cv2.VideoCapture(filepath)

    if not cap.isOpened():
        raise FileNotFoundError(f"Cannot open video file: {filepath}")

    fps = int(cap.get(cv2.CAP_PROP_FPS))
    ts = detection['ts']
    bb = detection['bb']

    # Calculate the frame number
    frame_number = ts * fps
    cap.set(cv2.CAP_PROP_POS_FRAMES, frame_number)

    ret, frame = cap.read()
    if not ret:
        raise ValueError(f"Failed to read frame at timestamp {ts} seconds")

    # Extract bounding box coordinates
    x1, y1, x2, y2 = bb

    # Crop the bounding box from the frame
    cropped_img = frame[y1:y2, x1:x2]

    # Convert the cropped image to a PIL Image
    cropped_img_pil = Image.fromarray(cv2.cvtColor(cropped_img, cv2.COLOR_BGR2RGB))

    # Convert PIL Image to base64 string
    buffered = io.BytesIO()
    cropped_img_pil.save(buffered, format="PNG")
    img_str = base64.b64encode(buffered.getvalue()).decode("utf-8")

    return img_str
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
