# Get bounding box of person from video
import cv2
from ultralytics import YOLO
import numpy as np
import json

YOLO_MODEL_PATH = "yolov8s.pt"
PROCESS_INTERVAL_SECONDS = 5

# Initialize YOLO model
model = YOLO(YOLO_MODEL_PATH)

def get_person_from_filepath(filepath: str)->str:
    print("Processing {}".format(filepath))
    cap = cv2.VideoCapture(filepath)
    fps = int(cap.get(cv2.CAP_PROP_FPS))  # Get frames per second
    frame_interval = fps * PROCESS_INTERVAL_SECONDS
    frame_count = 0
    detections = []

    while True:
        ret, frame = cap.read()
        if not ret:
            break

        if frame_count % frame_interval == 0:
            results = model(frame, imgsz=320)
            result = results[0]
            classes = np.array(result.boxes.cls.cpu(), dtype="int")
            bboxes = np.array(result.boxes.xyxy.cpu(), dtype="int")

            for cls, bbox in zip(classes, bboxes):
                if cls == 0:  # Class 0 is 'person' for YOLOv8
                    seconds = frame_count // fps
                    detections.append({"ts": seconds, "bb": bbox.tolist()})

        frame_count += 1

    cap.release()
    detections_json = json.dumps(detections)

    return detections_json