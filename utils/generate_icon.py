import requests
import json
import sys
from PIL import Image

def generate_icon(api_key: str, output_path: str = "app_icon.png"):
    url = "https://aistudio.baidu.com/llm/lmapi/v3/images/generations"
    
    payload = {
        "model": "ernie-image-turbo",
        "prompt": "A modern minimalist app icon for a voice recording application. Clean design with a microphone symbol in the center, soft gradient background in blue and purple tones, flat design style, suitable for desktop application, 512x512 pixels",
        "n": 1,
        "response_format": "url",
        "size": "512x512"
    }
    
    headers = {
        "Authorization": f"bearer {api_key}",
        "Content-Type": "application/json"
    }
    
    print("Generating icon...")
    response = requests.post(url, json=payload, headers=headers)
    
    if response.status_code == 200:
        result = response.json()
        if "data" in result and len(result["data"]) > 0:
            image_url = result["data"][0]["url"]
            print(f"Downloading from: {image_url}")
            
            img_response = requests.get(image_url)
            if img_response.status_code == 200:
                with open(output_path, "wb") as f:
                    f.write(img_response.content)
                print(f"Icon saved to: {output_path}")
                return True
            else:
                print(f"Failed to download image: {img_response.status_code}")
        else:
            print(f"No image data in response: {result}")
    else:
        print(f"API Error: {response.status_code} - {response.text}")
    
    return False

if __name__ == "__main__":
    api_key = "***********************"  # Replace with your actual API key
    output_path = sys.argv[1] if len(sys.argv) > 1 else "app_icon.png"
    
    if generate_icon(api_key, output_path):
        ico_path = output_path.replace(".png", ".ico")
        img = Image.open(output_path)
        img.save(ico_path, format="ICO", sizes=[(16,16),(32,32),(48,48),(256,256)])
        print(f"Icon converted to: {ico_path}")
