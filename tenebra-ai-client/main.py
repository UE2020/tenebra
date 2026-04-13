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
from pypdf import PdfReader
from fastapi import UploadFile, File

# Initialize FastAPI
app = FastAPI(title="Tenebra AI Client Backend")

# Configure Gemini
GEMINI_API_KEY = os.environ.get("GEMINI_API_KEY")
if not GEMINI_API_KEY:
    print("WARNING: GEMINI_API_KEY environment variable not set.")
else:
    genai.configure(api_key=GEMINI_API_KEY)

class ChatRequest(BaseModel):
    message: str
    image_base64: Optional[str] = None
    is_zoomed: Optional[bool] = False
    history: List[dict] = []
    width: Optional[int] = None
    height: Optional[int] = None
    model: Optional[str] = "gemini-3.1-flash-lite-preview"
    context: Optional[str] = None

SYSTEM_PROMPT = """
You are controlling a remote desktop server named 'Tenebra'.
You will receive a screenshot and a user goal. 
Analyze the screenshot and provide a JSON response containing 'reasoning', 'status', 'plan', and 'actions'.

Toolbox (Actions):
- {"type": "click_at", "x": x, "y": y, "button": 0|1|2, "clicks": 1|2|3}: Click at normalized coordinates. Use clicks: 2 for double-click, 3 for triple-click.
- {"type": "drag_and_drop", "x1": x1, "y1": y1, "x2": x2, "y2": y2}: Drag from 1 to 2.
- {"type": "scroll", "direction": "up"|"down", "amount": n}: Scroll the mouse wheel. n is the number of 'notches'. Note: 1 notch (amount: 1) is ~3 lines of text. Use amount: 5-10 for full page scrolls.
- {"type": "type_text", "text": "string"}: Type the specified string into the currently focused element.
- {"type": "press_shortcut", "keys": ["Key1", "Key2", ...]}: Press a shortcut. Use for formatting like ["ControlLeft", "b"] for bold.
- {"type": "wait", "ms": milliseconds}: Pause tool execution.
- {"type": "chat", "text": "message"}: Speak to user.
- {"type": "zoom", "x": x, "y": y, "scale": 2|3|4|5}: Zooms the camera into a specific patch of the screen centered at (x, y). Use when you need sub-pixel targeting accuracy on tiny objects.

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
3. PERFECT MEMORY: Check 'Actions performed' in history. If you have repeated the same action (e.g. clicking a dropdown) 3 times without a visible state change towards your goal, you MUST change your strategy (e.g. scroll to see if the menu is off-screen, or try a different approach). Never repeat a failing action more than thrice.
4. ATOMIC UI PRINCIPLE: Only interact with visible elements.
5. PREFER SHORTCUTS: Use 'MetaLeft', 'AltLeft', 'ControlLeft' for navigation.
6. TASK SUMMARY: When setting status to 'complete', you MUST include a final 'chat' action summarizing exactly what was accomplished and any relevant results for the user. Never end a task silently.
7. TRANSITION HALT: If UI changes significantly, set 'status' to 'continue' to re-calculate.
8. DRAG-AND-DROP ISOLATION: Dragging often causes UI layout shifts. If you perform a 'drag_and_drop', do NOT perform any other mouse-based actions in the same turn. Use 'status': 'continue' to re-observe the screen after the drag.
9. VISUAL STABILITY & PATIENCE: Before interacting, check for loading spinners, progress bars, or 'ghost' content. If a page is loading, use the 'wait' tool (2000-5000ms) or set 'status' to 'continue' to wait for stability. Never click on a 'Loading...' indicator.
10. IDEMPOTENT TYPING: If unsure about cursor placement, rewrite the whole block/word instead of trying to edit in-place. Prioritize indentation and formatting accuracy over speed.
11. FORMATTING SHORTCUTS: Prefer standard OS shortcuts (Ctrl+B, Ctrl+I, etc.) for formatting text rather than clicking UI menus.
12. EXPLORATORY SCROLLING: If a target element is not visible, you MUST use 'scroll' to explore the page. Do not assume a task is 'complete' until you have visually confirmed the target exists or you have exhausted the searchable area.
13. FOCUS BEFORE ACTION: Always perform a standard `click_at` (clicks: 1) on the specific window or container to set focus before performing a `scroll`, `type_text`, or right-click.
14. MULTI-CLICK SELECTION: Effectively select text using click counts:
    - Use `clicks: 2` (double-click) to select a single word.
    - Use `clicks: 3` (triple-click) to select an entire line of code or a paragraph.
15. SCROLLING PREFERENCE: Mouse-wheel `scroll` is often unreliable depending on cursor focus. For more robust navigation, use `press_shortcut` with `["PageDown"]` or `["PageUp"]` INSTEAD of using the `scroll` tool where possible.
16. SCROLL VERIFICATION & BOUNDARY TESTING: If you MUST use `scroll` and your next frame shows the screen did not move, the scroll failed. You MUST then fallback to ensuring focus (Rule 13) and immediately try scrolling in BOTH directions (up AND down) to test if you are at a boundary. Never endlessly retry a failed scroll direction without explicitly testing the opposite.
17. OFF-SCREEN MENU DETECTION: If you click a dropdown or menu and no options appear, assume the content is off-screen. Use 'scroll' (Rule 15) to locate the hidden UI elements instead of re-clicking the menu.
18. PRECISION TARGETING: When dealing with non-traditional UI elements like graph points or precise drawing nodes, you MUST use the 'zoom' tool to get a magnified patch of the screen. Do not attempt to guess coordinates of sub-elements on a full-screen view. When zoomed, the coordinates (0-1000) you output map ONLY to the magnified bounds. The system translates them back to absolute space. Zooming automatically resets to full-screen upon any interaction.

IMPORTANT: Respond ONLY with the JSON object.
"""

@app.post("/chat")
async def chat_endpoint(request: ChatRequest):
    if not GEMINI_API_KEY:
        raise HTTPException(status_code=500, detail="Gemini API Key missing")

    contents = [SYSTEM_PROMPT]
    
    # Add context if provided
    if request.context:
        contents.append(f"Additional Agent Context/Document Data:\n{request.context}")

    # Add history
    for msg in request.history:
        contents.append(f"{msg['role']}: {msg['content']}")

    # Add current message
    current_message = f"User Request/Goal: {request.message}"
    if request.width and request.height:
        current_message += f"\n(Current Screen Resolution: {request.width}x{request.height})"
        
    if getattr(request, 'is_zoomed', False):
        current_message += "\n\n⚠️ SYSTEM NOTICE: You are currently looking at a ZOOMED patch of the screen. Any coordinates you output will explicitly map relative to this patch, not the absolute screen!"
        
    contents.append(current_message)

    # Add current image if present
    if request.image_base64:
        try:
            image_data = base64.b64decode(request.image_base64.split(",")[-1])
            img = Image.open(io.BytesIO(image_data))
            contents.append(img)
        except Exception as e:
            raise HTTPException(status_code=400, detail=f"Invalid image data: {str(e)}")

    try:
        # Dynamically select model
        target_model_name = request.model or "gemini-3.1-flash-lite-preview"
        active_model = genai.GenerativeModel(target_model_name)
        response = active_model.generate_content(contents)
        text = response.text.strip()
        
        # Robustly extract JSON block
        def extract_json_block(t):
            start_idx = t.find('{')
            if start_idx == -1: return None
            brace_count = 0
            in_string = False
            escape = False
            for i in range(start_idx, len(t)):
                char = t[i]
                if escape:
                    escape = False; continue
                if char == '\\':
                    escape = True; continue
                if char == '"':
                    in_string = not in_string; continue
                if not in_string:
                    if char == '{': brace_count += 1
                    elif char == '}':
                        brace_count -= 1
                        if brace_count == 0: return t[start_idx:i+1]
            return None
            
        json_text = extract_json_block(text)
        if json_text:
            text = json_text
        else:
            print(f"DEBUG: No valid JSON block found in response: {text}")
            raise ValueError("No valid JSON structure found in response")
            
        import json
        import ast
        try:
            result = json.loads(text)
        except json.JSONDecodeError:
            # Fallback for single-quote JSON or other minor formatting issues
            try:
                result = ast.literal_eval(text)
                if not isinstance(result, dict):
                    raise ValueError("Parsed object is not a dictionary")
            except Exception as e:
                print(f"DEBUG: Both json.loads and ast.literal_eval failed for: {text}")
                raise ValueError(f"Failed to parse model output as JSON: {str(e)}")
        
        # Include usage metadata for spend tracking
        if hasattr(response, 'usage_metadata'):
            result['usage'] = {
                'prompt_tokens': response.usage_metadata.prompt_token_count,
                'candidates_tokens': response.usage_metadata.candidates_token_count,
                'total_tokens': response.usage_metadata.total_token_count
            }
            
        return result
    except Exception as e:
        print(f"Gemini Error: {e}")
        return {
            "status": "error",
            "plan": f"Error occurred: {str(e)}",
            "actions": [{"type": "chat", "text": f"Error: {str(e)}"}]
        }

# Endpoint to upload context files (PDF)
@app.post("/upload")
async def upload_file(file: UploadFile = File(...)):
    if not file.filename.endswith(".pdf"):
        raise HTTPException(status_code=400, detail="Only PDF files are supported")
    
    try:
        reader = PdfReader(file.file)
        text = ""
        for page in reader.pages:
            text += page.extract_text() + "\n"
        
        return {"filename": file.filename, "content": text[:50000]} # Limit to 50k chars for sanity
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"PDF error: {str(e)}")

# Serve static files
app.mount("/", StaticFiles(directory="static", html=True), name="static")

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
