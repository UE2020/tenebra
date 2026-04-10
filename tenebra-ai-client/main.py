import os
import json
import base64
from typing import List, Optional
from fastapi import FastAPI, HTTPException, Request
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel
import google.generativeai as genai
from PIL import Image
import io

# Initialize FastAPI
app = FastAPI(title="Tenebra AI Client Backend")

# Configure Gemini
GEMINI_API_KEY = os.environ.get("GEMINI_API_KEY")
if not GEMINI_API_KEY:
    print("WARNING: GEMINI_API_KEY environment variable not set.")
else:
    genai.configure(api_key=GEMINI_API_KEY)

model = genai.GenerativeModel("gemini-3.1-flash-lite-preview")

class ChatRequest(BaseModel):
    message: str
    image_base64: Optional[str] = None
    history: List[dict] = []
    width: Optional[int] = None
    height: Optional[int] = None

SYSTEM_PROMPT = """
You are controlling a remote desktop server named 'Tenebra'.
You will receive a screenshot and a user goal. 
Analyze the screenshot and provide a JSON response containing 'reasoning', 'status', 'plan', and 'actions'.

Toolbox (Actions):
- {"type": "click_at", "x": x, "y": y, "button": 0|1|2}: Click at normalized coordinates.
- {"type": "drag_and_drop", "x1": x1, "y1": y1, "x2": x2, "y2": y2}: Drag from 1 to 2.
- {"type": "scroll", "direction": "up"|"down", "amount": n}: Scroll the mouse wheel. n is the number of 'notches'. Note: 1 notch (amount: 1) is ~3 lines of text. Use amount: 5-10 for full page scrolls.
- {"type": "type_text", "text": "string"}: Type the specified string into the currently focused element.
- {"type": "press_shortcut", "keys": ["Key1", "Key2", ...]}: Press a shortcut.
- {"type": "wait", "ms": milliseconds}: Pause tool execution.
- {"type": "chat", "text": "message"}: Speak to user.

Response Schema:
{
  "reasoning": "Detailed reflection on the current state. Compare the previous frame with this one. Did the last action succeed? What visual cues confirm this?",
  "status": "continue" | "complete" | "error",
  "plan": "Current high-level goal step.",
  "actions": [...]
}

Rules:
1. PERSISTENT VISUAL MEMORY: If a task spans multiple scroll positions (e.g. Chart at top, Question at bottom), you MUST explicitly record key facts in your 'reasoning' before scrolling. Trust your own history as a source of truth for off-screen data.
2. OBSERVE & REFLECT: Before planning, compare current vs previous frames. Assume success if a dialog disappeared after 'Save'.
3. PERFECT MEMORY: Check 'Actions performed' in history to avoid repeating text or clicks.
4. ATOMIC UI PRINCIPLE: Only interact with visible elements.
5. PREFER SHORTCUTS: Use 'MetaLeft', 'AltLeft', 'ControlLeft' for navigation.
6. TASK SUMMARY: On 'complete', use 'chat' to provide a summary.
7. TRANSITION HALT: If UI changes significantly, set 'status' to 'continue' to re-calculate.
8. DRAG-AND-DROP ISOLATION: Dragging often causes UI layout shifts. If you perform a 'drag_and_drop', do NOT perform any other mouse-based actions in the same turn. Use 'status': 'continue' to re-observe the screen after the drag.

IMPORTANT: Respond ONLY with the JSON object.
"""

@app.post("/chat")
async def chat_endpoint(request: ChatRequest):
    if not GEMINI_API_KEY:
        raise HTTPException(status_code=500, detail="Gemini API Key missing")

    contents = [SYSTEM_PROMPT]
    
    # Add history
    for msg in request.history:
        contents.append(f"{msg['role']}: {msg['content']}")

    # Add current message
    current_message = f"User Request/Goal: {request.message}"
    if request.width and request.height:
        current_message += f"\n(Current Screen Resolution: {request.width}x{request.height})"
    contents.append(current_message)

    # Add image if present
    if request.image_base64:
        try:
            image_data = base64.b64decode(request.image_base64.split(",")[-1])
            img = Image.open(io.BytesIO(image_data))
            contents.append(img)
        except Exception as e:
            raise HTTPException(status_code=400, detail=f"Invalid image data: {str(e)}")

    try:
        response = model.generate_content(contents)
        text = response.text.strip()
        
        # Clean up Markdown JSON blocks if present
        if text.startswith("```json"):
            text = text[7:-3].strip()
        elif text.startswith("```"):
            text = text[3:-3].strip()
            
        result = json.loads(text)
        return result
    except Exception as e:
        print(f"Gemini Error: {e}")
        return {
            "status": "error",
            "plan": f"Error occurred: {str(e)}",
            "actions": [{"type": "chat", "text": f"Error: {str(e)}"}]
        }

# Serve static files
app.mount("/", StaticFiles(directory="static", html=True), name="static")

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
