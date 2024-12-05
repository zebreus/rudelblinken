from pathlib import Path
from PIL import Image
import numpy as np
import colorsys

# I dont have the tooling nor the patience to do this properly, so this must do.

INPUT_FILE = Path("./logo.png")
OUTPUT_FILE = Path("./logo.webp")
FRAME_RATE = 15 # frames per second
DURATION = 3 # seconds
SHIFTS = 1
BLINKS = 3 

rgb_to_hsv = np.vectorize(colorsys.rgb_to_hsv)
hsv_to_rgb = np.vectorize(colorsys.hsv_to_rgb)
def shift_hue(arr, hout, factor):
    r, g, b, a = np.rollaxis(arr, axis=-1)
    h, s, v = rgb_to_hsv(r, g, b)
    h = hout
    v *= factor
    r, g, b = hsv_to_rgb(h, s, v)
    arr = np.dstack((r, g, b, a))
    return arr

def only_red(arr):
    r, g, b, a = np.rollaxis(arr, axis=-1)
    g = 0.0
    b = 0.0
    h, s, v = rgb_to_hsv(r, g, b)
    r, g, b = hsv_to_rgb(h, s, v)
  
    arr = np.dstack((r, g, b, a))
    return arr

def adjust_brightness(arr, factor):
    r, g, b, a = np.rollaxis(arr, axis=-1)
    h, s, v = rgb_to_hsv(r, g, b)
    v *= factor
    r, g, b = hsv_to_rgb(h, s, v)
  
    arr = np.dstack((r, g, b, a))
    return arr

# Find all '.webp'-files in the input directory and sort them by filename
input_logo = Image.open(INPUT_FILE).convert('RGBA')
arr = np.array(np.asarray(input_logo).astype('float'))
arr = only_red(arr).astype('float')

# Calculate the duration per frame
frame_duration_ms = round(1000 / FRAME_RATE) # milliseconds



# Load each frame as a PIL.Image object and store it in a list
frames = []

for i in range(FRAME_RATE * DURATION):
    elapsed_time = i / FRAME_RATE
    hue = elapsed_time * (SHIFTS / DURATION) * 1.0
    brightness = 0.5 + 0.5 * np.sin(2 * np.pi * (BLINKS / DURATION) * elapsed_time)
    print(hue, brightness)
    new_img = Image.fromarray(shift_hue(arr, hue, brightness).astype('uint8'), 'RGBA')
    frames.append(new_img)

frames[0].save(OUTPUT_FILE, save_all=True, append_images=frames[1:], duration=1000/FRAME_RATE, loop=0)
# Save the first frame as the output image, with all remaining frames appended
