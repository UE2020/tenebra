let conn = null;
let orderedChannel = null;
let video = document.getElementById('remoteVideo');
let chatMessages = document.getElementById('chatMessages');
let statusSpan = document.getElementById('status');
let resizer = document.getElementById('resizer');
let menuToggle = document.getElementById('menuToggle');
let sidebar = document.querySelector('.sidebar');
let glassContainer = document.querySelector('.glass-container');
let modelSelect = document.getElementById('modelSelect');
let totalSpend = 0;

const PRICING = {
    "gemini-3.1-flash-lite-preview": { input: 0.25, output: 1.50 },
    "gemini-3-flash-preview": { input: 0.50, output: 3.00 }
};
let history = [];
let aiLoopActive = false;
let stopRequested = false;
let videoLoaded = false;
let currentPDFContext = "";
let uploadedFiles = [];

// UI Elements
const connectBtn = document.getElementById('connectBtn');
const sendBtn = document.getElementById('sendBtn');
const stopBtn = document.getElementById('stopBtn');
const userInput = document.getElementById('userInput');
const addressInput = document.getElementById('addressInput');
const passwordInput = document.getElementById('passwordInput');
const aiThought = document.getElementById('aiThought');
const scanningBar = document.querySelector('.scanning-bar');
const uploadBtn = document.getElementById('uploadBtn');
const fileInput = document.getElementById('fileInput');
const contextBadge = document.getElementById('contextBadge');

const KEY_MAP = {
    'a': 'KeyA', 'b': 'KeyB', 'c': 'KeyC', 'd': 'KeyD', 'e': 'KeyE', 'f': 'KeyF', 'g': 'KeyG', 'h': 'KeyH',
    'i': 'KeyI', 'j': 'KeyJ', 'k': 'KeyK', 'l': 'KeyL', 'm': 'KeyM', 'n': 'KeyN', 'o': 'KeyO', 'p': 'KeyP',
    'q': 'KeyQ', 'r': 'KeyR', 's': 'KeyS', 't': 'KeyT', 'u': 'KeyU', 'v': 'KeyV', 'w': 'KeyW', 'x': 'KeyX',
    'y': 'KeyY', 'z': 'KeyZ',
    '0': 'Digit0', '1': 'Digit1', '2': 'Digit2', '3': 'Digit3', '4': 'Digit4', '5': 'Digit5', '6': 'Digit6',
    '7': 'Digit7', '8': 'Digit8', '9': 'Digit9',
    ' ': 'Space', '.': 'Period', ',': 'Comma', '/': 'Slash', ';': 'Semicolon', "'": 'Quote', '[': 'BracketLeft',
    ']': 'BracketRight', '\\': 'Backslash', '-': 'Minus', '=': 'Equal', '`': 'Backquote', '\n': 'Enter', '\t': 'Tab'
};

const SHIFT_MAP = {
    'A': 'KeyA', 'B': 'KeyB', 'C': 'KeyC', 'D': 'KeyD', 'E': 'KeyE', 'F': 'KeyF', 'G': 'KeyG', 'H': 'KeyH',
    'I': 'KeyI', 'J': 'KeyJ', 'K': 'KeyK', 'L': 'KeyL', 'M': 'KeyM', 'N': 'KeyN', 'O': 'KeyO', 'P': 'KeyP',
    'Q': 'KeyQ', 'R': 'KeyR', 'S': 'KeyS', 'T': 'KeyT', 'U': 'KeyU', 'V': 'KeyV', 'W': 'KeyW', 'X': 'KeyX',
    'Y': 'KeyY', 'Z': 'KeyZ',
    '!': 'Digit1', '@': 'Digit2', '#': 'Digit3', '$': 'Digit4', '%': 'Digit5', '^': 'Digit6', '&': 'Digit7',
    '*': 'Digit8', '(': 'Digit9', ')': 'Digit0', '_': 'Minus', '+': 'Equal', '{': 'BracketLeft', '}': 'BracketRight',
    '|': 'Backslash', ':': 'Semicolon', '"': 'Quote', '<': 'Comma', '>': 'Period', '?': 'Slash', '~': 'Backquote'
};

// Load persisted values
window.addEventListener('DOMContentLoaded', () => {
    addressInput.value = localStorage.getItem('tenebra_address') || '';
    passwordInput.value = localStorage.getItem('tenebra_password') || '';
});

async function connect() {
    const address = addressInput.value;
    const password = passwordInput.value;
    if (!address) return alert("Please enter server address");

    // Persist values
    localStorage.setItem('tenebra_address', address);
    localStorage.setItem('tenebra_password', password);

    statusSpan.innerText = "Connecting...";
    connectBtn.disabled = true;

    conn = new RTCPeerConnection({ iceServers: [{ urls: "stun:stun.l.google.com:19302" }] });
    orderedChannel = conn.createDataChannel("ordered-input", { ordered: true });

    // Start stats loop
    const statsInterval = setInterval(async () => {
        if (!conn || conn.connectionState === 'closed') {
            clearInterval(statsInterval);
            return;
        }
        const stats = await conn.getStats();
        stats.forEach(report => {
            if (report.type === 'inbound-rtp' && report.kind === 'video') {
                document.getElementById('fps').innerText = `${Math.round(report.framesPerSecond || 0)} fps`;
            }
            if (report.type === 'remote-candidate' || report.type === 'candidate-pair') {
                if (report.currentRoundTripTime) {
                    document.getElementById('latency').innerText = `${Math.round(report.currentRoundTripTime * 1000)} ms`;
                }
            }
        });
    }, 1000);

    conn.oniceconnectionstatechange = () => {
        statusSpan.innerText = conn.iceConnectionState;
        if (['failed', 'disconnected', 'closed'].includes(conn.iceConnectionState)) {
            connectBtn.disabled = false;
            aiLoopActive = false;
        }
    };

    conn.ontrack = (event) => {
        if (event.track.kind === "video") {
            video.srcObject = event.streams[0];
            video.onloadedmetadata = () => { videoLoaded = true; statusSpan.innerText = "Connected"; };
            video.play();
        }
    };

    conn.onicecandidate = async (event) => {
        if (!event.candidate) {
            const offerPayload = { password, show_mouse: true, low_power_mode: true, offer: btoa(JSON.stringify(conn.localDescription)) };
            try {
                const response = await fetch(`https://${address}/offer`, {
                    method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(offerPayload)
                });
                if (response.ok) {
                    const data = await response.json();
                    const answer = JSON.parse(atob(data.Offer));
                    await conn.setRemoteDescription(new RTCSessionDescription(answer));
                } else {
                    alert(`Connection Error: ${(await response.json()).Error || response.statusText}`);
                    connectBtn.disabled = false;
                }
            } catch (e) { alert(`Failed to signal: ${e.message}`); connectBtn.disabled = false; }
        }
    };

    conn.addTransceiver("video", { direction: "recvonly" });
    const offer = await conn.createOffer();
    await conn.setLocalDescription(offer);
}

function addMessage(role, text) {
    const msgDiv = document.createElement('div');
    msgDiv.className = `message ${role}`;
    msgDiv.innerText = text;
    chatMessages.appendChild(msgDiv);
    chatMessages.scrollTop = chatMessages.scrollHeight;
    history.push({ role, content: text });
    if (history.length > 20) history.shift();
}

// Auto-resize textarea
userInput.addEventListener('input', () => {
    userInput.style.height = 'auto';
    userInput.style.height = (userInput.scrollHeight) + 'px';
});

// File Upload Logic
uploadBtn.onclick = () => fileInput.click();
fileInput.onchange = async (e) => {
    const file = e.target.files[0];
    if (!file) return;

    const originalText = contextBadge.innerText;
    contextBadge.innerText = "Extracting...";
    
    const formData = new FormData();
    formData.append("file", file);

    try {
        const response = await fetch("/upload", { method: "POST", body: formData });
        if (response.ok) {
            const data = await response.json();
            // Append rather than overwrite
            currentPDFContext += `\n--- Context from ${data.filename} ---\n${data.content}\n`;
            uploadedFiles.push(data.filename);
            
            contextBadge.innerText = `${uploadedFiles.length} files loaded`;
            contextBadge.style.color = "var(--accent)";
            contextBadge.title = uploadedFiles.join(", ");
        } else {
            alert("Upload failed: " + (await response.json()).detail);
            contextBadge.innerText = originalText;
        }
    } catch (err) {
        alert("Upload error: " + err.message);
        contextBadge.innerText = originalText;
    }
    // Clear input so same file can be uploaded again if needed
    fileInput.value = "";
};

// Add clear context capability
contextBadge.onclick = () => {
    if (uploadedFiles.length > 0 && confirm("Clear all uploaded context?")) {
        currentPDFContext = "";
        uploadedFiles = [];
        contextBadge.innerText = "No context";
        contextBadge.style.color = "";
        contextBadge.title = "";
    }
};

function logAction(type, info) {
    const entry = document.createElement('div');
    entry.className = `action-chip ${type}`;
    entry.innerText = `> ${type.toUpperCase()}`; // Clean terminal style
    chatMessages.appendChild(entry);
    chatMessages.scrollTop = chatMessages.scrollHeight;
}

function logThought(text) {
    const entry = document.createElement('div');
    entry.className = `thought-entry`;
    entry.innerText = text;
    chatMessages.appendChild(entry);
    chatMessages.scrollTop = chatMessages.scrollHeight;
}

async function captureFrame() {
    if (!videoLoaded) return null;
    const canvas = document.createElement('canvas');
    canvas.width = video.videoWidth; canvas.height = video.videoHeight;
    canvas.getContext('2d').drawImage(video, 0, 0);
    return canvas.toDataURL('image/jpeg', 0.8);
}

async function handleAction(action) {
    if (stopRequested) return;
    logAction(action.type, JSON.stringify(action));

    switch (action.type) {
        case 'click_at':
            await executeClickAt(action.x, action.y, action.button || 0, action.clicks || 1);
            break;
        case 'drag_and_drop':
            await executeDragAndDrop(action.x1, action.y1, action.x2, action.y2);
            break;
        case 'scroll':
            await executeScroll(action.direction, action.amount || 1);
            break;
        case 'type_text':
            await executeTypeText(action.text);
            break;
        case 'press_shortcut':
            await executePressShortcut(action.keys);
            break;
        case 'keydown':
        case 'keyup':
            sendPacket(action);
            break;
        case 'wait':
            const ms = action.ms || 500;
            aiThought.innerText = `Waiting for ${ms}ms...`;
            await new Promise(r => setTimeout(r, ms));
            break;
        case 'chat':
            addMessage('assistant', action.text);
            break;
    }
}

function normalizeKey(key) {
    const maps = {
        'Meta': 'MetaLeft',
        'Win': 'MetaLeft',
        'Command': 'MetaLeft',
        'Alt': 'AltLeft',
        'Control': 'ControlLeft',
        'Ctrl': 'ControlLeft',
        'Shift': 'ShiftLeft'
    };
    return maps[key] || key;
}

async function executePressShortcut(keys) {
    const normalizedKeys = keys.map(normalizeKey);
    for (const key of normalizedKeys) {
        sendPacket({ type: 'keydown', key: key });
    }
    await new Promise(r => setTimeout(r, 50));
    for (const key of [...normalizedKeys].reverse()) {
        sendPacket({ type: 'keyup', key: key });
    }
}

async function executeClickAt(nx, ny, button = 0, clicks = 1) {
    const x = Math.round((nx / 1000) * video.videoWidth);
    const y = Math.round((ny / 1000) * video.videoHeight);
    sendPacket({ type: 'mousemoveabs', x, y });
    await new Promise(r => setTimeout(r, 100));
    
    for (let i = 0; i < clicks; i++) {
        sendPacket({ type: 'mousedown', button: button });
        await new Promise(r => setTimeout(r, 50));
        sendPacket({ type: 'mouseup', button: button });
        if (clicks > 1 && i < clicks - 1) {
            await new Promise(r => setTimeout(r, 100)); // Delay between clicks
        }
    }
}

async function executeDragAndDrop(nx1, ny1, nx2, ny2) {
    const video = document.getElementById('remoteVideo');
    const x1 = Math.round((nx1 / 1000) * video.videoWidth);
    const y1 = Math.round((ny1 / 1000) * video.videoHeight);
    const x2 = Math.round((nx2 / 1000) * video.videoWidth);
    const y2 = Math.round((ny2 / 1000) * video.videoHeight);

    // 1. Move to start
    sendPacket({ type: 'mousemoveabs', x: x1, y: y1 });
    await new Promise(r => setTimeout(r, 100));

    // 2. Click down
    sendPacket({ type: 'mousedown', button: 0 });
    await new Promise(r => setTimeout(r, 200)); // Crucial "wait" to engage windows drag

    // 3. Smooth Interpolation (LERP) to destination
    const steps = 10;
    for (let i = 1; i <= steps; i++) {
        const curX = Math.round(x1 + (x2 - x1) * (i / steps));
        const curY = Math.round(y1 + (y2 - y1) * (i / steps));
        sendPacket({ type: 'mousemoveabs', x: curX, y: curY });
        await new Promise(r => setTimeout(r, 30)); // 300ms total travel time
    }

    // 4. Release mouse
    await new Promise(r => setTimeout(r, 100));
    sendPacket({ type: 'mouseup', button: 0 });
}

async function executeScroll(direction, amount) {
    // Standard convention: Negative for Down (toward user), Positive for Up (away)
    const deltaY = direction === 'down' ? 120 * amount : -120 * amount;
    sendPacket({ type: 'wheel', x: 0, y: deltaY });
}

async function executeTypeText(text) {
    const start = performance.now();
    for (const char of text) {
        if (stopRequested) break;
        let code = KEY_MAP[char];
        let shift = false;
        if (!code && SHIFT_MAP[char]) { code = SHIFT_MAP[char]; shift = true; }

        if (code) {
            if (shift) sendPacket({ type: 'keydown', key: 'ShiftLeft' });
            sendPacket({ type: 'keydown', key: code });
            sendPacket({ type: 'keyup', key: code });
            if (shift) sendPacket({ type: 'keyup', key: 'ShiftLeft' });
        }
        await new Promise(r => setTimeout(r, 100 / 3)); // ~120 WPM
    }
    const end = performance.now();
    console.log(`Typed "${text}" in ${Math.round(end - start)}ms`);
}

function sendPacket(packet) {
    if (orderedChannel && orderedChannel.readyState === 'open') {
        orderedChannel.send(JSON.stringify(packet));
    }
}

async function startAutonomousLoop(goal) {
    if (aiLoopActive) return;
    aiLoopActive = true;
    stopRequested = false;
    scanningBar.classList.add('active');

    while (aiLoopActive && !stopRequested) {
        aiThought.innerText = "Settling UI...";
        // Observation Settling Delay (Wait for animations/transfers to finish)
        await new Promise(r => setTimeout(r, 1200)); 
        
        aiThought.innerText = "Observing screen...";
        const screenshot = await captureFrame();

        try {
            const response = await fetch('/chat', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    message: goal,
                    image_base64: screenshot,
                    history: history.slice(-20),
                    width: video.videoWidth,
                    height: video.videoHeight,
                    model: modelSelect.value,
                    context: currentPDFContext
                })
            });

            const result = await response.json();

            // Track API Spend
            if (result.usage) {
                const rates = PRICING[modelSelect.value] || PRICING["gemini-3.1-flash-lite-preview"];
                const inputCost = (result.usage.prompt_tokens / 1000000) * rates.input;
                const outputCost = (result.usage.candidates_tokens / 1000000) * rates.output;
                totalSpend += (inputCost + outputCost);
                document.getElementById('spend').innerText = `$${totalSpend.toFixed(4)}`;
            }

            if (result.reasoning) {
                logThought(result.reasoning);
                // PERFECT MEMORY: Include Reasoning, Plan, and EXACT Actions in history
                history.push({
                    role: 'assistant',
                    content: `Reasoning: ${result.reasoning}\nPlan: ${result.plan}\nActions performed: ${JSON.stringify(result.actions)}`
                });
            }

            aiThought.innerText = result.plan || "Executing...";

            for (const action of result.actions) {
                if (stopRequested) break;
                await handleAction(action);
            }

            if (result.status === 'complete') {
                const chatAction = result.actions.find(a => a.type === 'chat');
                if (!chatAction && result.reasoning) {
                    addMessage('assistant', `Task complete. Summary: ${result.reasoning}`);
                }
                aiLoopActive = false;
                break;
            }

            if (result.status === 'error') {
                aiLoopActive = false;
                break;
            }

            // Loop delay
            await new Promise(r => setTimeout(r, 1500));
        } catch (e) {
            addMessage('assistant', "Loop error: " + e.message);
            aiLoopActive = false;
        }
    }

    aiLoopActive = false;
    scanningBar.classList.remove('active');
    aiThought.innerText = stopRequested ? "Agent stopped." : "Task finished.";
}

// Event Listeners
connectBtn.onclick = connect;
stopBtn.onclick = () => { stopRequested = true; aiLoopActive = false; };
sendBtn.onclick = () => {
    const text = userInput.value; if (!text) return;
    addMessage('user', text);
    userInput.value = '';
    startAutonomousLoop(text);
};
userInput.onkeydown = (e) => { 
    if (e.key === 'Enter' && !e.shiftKey) { 
        e.preventDefault();
        sendBtn.click(); 
    } 
};

// Resizer Logic
let isResizing = false;

resizer.addEventListener('mousedown', (e) => {
    isResizing = true;
    resizer.classList.add('active');
    document.body.style.cursor = 'col-resize';
});

document.addEventListener('mousemove', (e) => {
    if (!isResizing) return;

    const containerWidth = glassContainer.offsetWidth;
    const containerHeight = glassContainer.offsetHeight;

    // Check if we are in vertical mode (stacked)
    const isVertical = window.innerWidth <= 800;

    if (isVertical) {
        const newVideoHeight = e.clientY;
        if (newVideoHeight > 100 && newVideoHeight < containerHeight - 100) {
            glassContainer.style.gridTemplateRows = `${newVideoHeight}px auto 1fr`;
            glassContainer.style.gridTemplateColumns = '1fr';
        }
    } else {
        const newTerminalWidth = containerWidth - e.clientX;
        // Constraints
        if (newTerminalWidth > 300 && newTerminalWidth < 800) {
            const sidebarWidth = window.innerWidth > 1024 ? '280px' : '0px'; // Account for hidden sidebar
            glassContainer.style.gridTemplateColumns = `${sidebarWidth} 1fr auto ${newTerminalWidth}px`;
        }
    }
});

document.addEventListener('mouseup', () => {
    isResizing = false;
    resizer.classList.remove('active');
    document.body.style.cursor = 'default';
});

// Sidebar Toggle
menuToggle.onclick = () => {
    sidebar.classList.toggle('active');
};
