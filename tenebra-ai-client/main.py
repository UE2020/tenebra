import os
import json
import base64

import re
import uvicorn
from typing import List, Optional
from fastapi import FastAPI, HTTPException, UploadFile, File
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field
from google import genai
from google.genai import types

from pypdf import PdfReader

class Action(BaseModel):
    type: str
    x: Optional[int] = None
    y: Optional[int] = None
    button: Optional[int] = None
    clicks: Optional[int] = None
    x1: Optional[int] = None
    y1: Optional[int] = None
    x2: Optional[int] = None
    y2: Optional[int] = None
    direction: Optional[str] = None
    amount: Optional[int] = None
    text: Optional[str] = None
    keys: Optional[List[str]] = None
    ms: Optional[int] = None
    scale: Optional[int] = None

class AgentResponse(BaseModel):
    reasoning: Optional[str] = Field(description="Detailed reflection on the current state")
    status: str = Field(description="continue | complete | error")
    plan: str
    actions: List[Action]

# Initialize FastAPI
app = FastAPI(title="Tenebra AI Client Backend")

# Configure Gemini
GEMINI_API_KEY = os.environ.get("GEMINI_API_KEY")
client = None
if not GEMINI_API_KEY:
    print("WARNING: GEMINI_API_KEY environment variable not set.")
else:
    client = genai.Client(api_key=GEMINI_API_KEY)

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
- {"type": "click_at", "x": x, "y": y, "button": 0|1|2, "clicks": 1|2|3}: Click at normalized coordinates (0 to 1000). button uses standard JS codes (0: Left, 1: Middle, 2: Right). ALWAYS use 0 for standard clicks! Use clicks: 2 for double-click.
- {"type": "drag_and_drop", "x1": x1, "y1": y1, "x2": x2, "y2": y2}: Drag from 1 to 2 using normalized coordinates (0 to 1000).
- {"type": "scroll", "x": x, "y": y, "direction": "up"|"down", "amount": n}: Scroll the mouse wheel at normalized coordinates (0 to 1000). n is the number of 'notches'. 1 notch is about 3 lines.
- {"type": "type_text", "text": "string"}: Type the specified string into the currently focused element.
- {"type": "press_shortcut", "keys": ["Key1", "Key2", ...]}: Press a shortcut. Use explicit W3C key codes like ["ControlLeft", "KeyB"] for bold.
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

    contents = []
    
    # 1. Build an intermediate merged list to enforce alternating roles
    merged = []
    
    # Add context if provided
    if request.context:
        merged.append({"role": "user", "parts": [types.Part.from_text(text=f"Additional Agent Context/Document Data:\n{request.context}")]})
        merged.append({"role": "model", "parts": [types.Part.from_text(text="Context recorded. I will use this information to accomplish tasks.")]})

    # Add history
    for msg in request.history:
        role = "user" if msg.get('role') == "user" else "model"
        
        parts = [types.Part.from_text(text=msg.get('content', ''))]
        if msg.get('image_base64'):
            try:
                b64_str = msg['image_base64'].split(",")[-1]
                mime_type = "image/jpeg"
                if "," in msg['image_base64']:
                    header = msg['image_base64'].split(",")[0]
                    if "data:" in header and ";" in header:
                        mime_type = header.split(";")[0].replace("data:", "")
                image_data = base64.b64decode(b64_str)
                parts.append(types.Part.from_bytes(data=image_data, mime_type=mime_type))
            except Exception as e:
                pass # Ignore invalid images in history

        if not merged:
            merged.append({"role": role, "parts": parts})
        else:
            if role == merged[-1]["role"]:
                if role == "model":
                    # Two models in a row (loop occurred). Inject synthetic user observation.
                    merged.append({"role": "user", "parts": [types.Part.from_text(text="[System Observation]: Action sequence executed. Outputting new state.")]})
                    merged.append({"role": "model", "parts": parts})
                else:
                    # Two users in a row. Merge content.
                    merged[-1]["parts"].append(types.Part.from_text(text="\n\n"))
                    merged[-1]["parts"].extend(parts)
            else:
                merged.append({"role": role, "parts": parts})

    # Add current message
    current_message = f"Current Active Goal: {request.message}"
    if request.width and request.height:
        current_message += f"\n(Current Screen Resolution: {request.width}x{request.height})"
    if getattr(request, 'is_zoomed', False):
        current_message += "\n\n⚠️ SYSTEM NOTICE: You are currently looking at a ZOOMED patch of the screen. Any coordinates you output will explicitly map relative to this patch, not the absolute screen!"
        
    current_parts = [types.Part.from_text(text=current_message)]

    # Add current image if present
    if request.image_base64:
        try:
            b64_str = request.image_base64.split(",")[-1]
            mime_type = "image/jpeg"
            if "," in request.image_base64:
                header = request.image_base64.split(",")[0]
                if "data:" in header and ";" in header:
                    mime_type = header.split(";")[0].replace("data:", "")
            
            image_data = base64.b64decode(b64_str)
            current_parts.append(types.Part.from_bytes(data=image_data, mime_type=mime_type))
        except Exception as e:
            raise HTTPException(status_code=400, detail=f"Invalid image data: {str(e)}")

    # Safely append current turn
    if merged and merged[-1]["role"] == "user":
        merged[-1]["parts"].extend(current_parts)
    else:
        merged.append({"role": "user", "parts": current_parts})

    contents = [types.Content(role=m["role"], parts=m["parts"]) for m in merged]

    try:
        # Dynamically select model
        target_model_name = request.model or "gemini-3.1-flash-lite-preview"
        
        if not client:
            raise HTTPException(status_code=500, detail="Gemini client not initialized")
            
        response = client.models.generate_content(
            model=target_model_name,
            contents=contents,
            config=types.GenerateContentConfig(
                system_instruction=SYSTEM_PROMPT,
                response_mime_type="application/json",
                response_schema=AgentResponse,
                tools=[{"google_search": {}}],
            )
        )
        
        if response.parsed:
            result = response.parsed.model_dump(exclude_none=True)
        else:
            if not response.text:
                raise HTTPException(
                    status_code=500, 
                    detail="Model returned an empty response. Check safety filters or model settings."
                )
            text = response.text.strip()
            # Clean markdown codeblocks
            if text.startswith("```"):
                text = re.sub(r"^```(?:json)?", "", text)
                text = re.sub(r"```$", "", text).strip()
            result = json.loads(text)

        
        # Include usage metadata for spend tracking
        if hasattr(response, 'usage_metadata') and response.usage_metadata:
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
    uvicorn.run(app, host="0.0.0.0", port=8000)
