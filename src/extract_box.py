import cv2
from PIL import Image
import io
import base64
from typing import List, Dict

def extract_box_as_b64(filepath: str, detection: Dict[str, List[int]]) -> str:
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
        cap.release()
        raise ValueError(f"Failed to read frame at timestamp {ts} seconds")

    # Extract bounding box coordinates
    x1, y1, x2, y2 = bb

    # Crop the bounding box from the frame
    cropped_img = frame[y1:y2, x1:x2]

    # Convert the cropped image to a PIL Image
    cropped_img_pil = Image.fromarray(cv2.cvtColor(cropped_img, cv2.COLOR_BGR2RGB))

    # Convert PIL Image to base64
    with io.BytesIO() as output:
        cropped_img_pil.save(output, format="PNG")
        b64_data = base64.b64encode(output.getvalue()).decode()

    cap.release()
    return b64_data
