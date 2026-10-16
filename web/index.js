window.onerror = (message, source, lineno, colno, error) => {
    alert(`An error occured at ${lineno}:${colno}: ${message}`);
    window.location.href = window.location.origin + window.location.pathname;
    return false;
}

const url = new URL(window.location.href);

function shallowEqual(a, b) {
    if (a === b) return true;
    if (a == null || b == null) return false;

    const keysA = Object.keys(a);
    const keysB = Object.keys(b);
    if (keysA.length !== keysB.length) return false;

    for (const key of keysA) {
        if (a[key] !== b[key]) {
            return false;
        }
    }
    return true;
}

function sleep(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

function distance(x1, y1, x2, y2) {
    return Math.sqrt((x2 - x1) ** 2 + (y2 - y1) ** 2);
}

class Checkbox {
    constructor(label, checked = false) {
        this.inner = document.createElement("label");
        this.inner.style.marginBottom = "var(--pico-spacing)";

        this.checkbox = document.createElement("input");
        this.checkbox.type = "checkbox";
        this.checked = checked;
        this.inner.appendChild(this.checkbox);

        this.label = document.createTextNode(label);
        this.inner.appendChild(this.label);
    }

    get checked() {
        return this.checkbox.checked;
    }

    set checked(value) {
        return this.checkbox.checked = value;
    }

    get disabled() {
        return this.checkbox.disabled;
    }

    set disabled(value) {
        return this.checkbox.disabled = value;
    }
}

class Range {
    constructor(label, min, max, value, step) {
        this.inner = document.createElement("label");
        this.inner.style.marginBottom = "var(--pico-spacing)";

        this.label = document.createTextNode(label);
        this.inner.appendChild(this.label);

        this.range = document.createElement("input");
        this.range.type = "range";
        this.range.min = min;
        this.range.max = max;
        this.value = value;
        this.range.step = step;
        this.range.style.marginBottom = "0";
        this.inner.appendChild(this.range);
    }

    get min() {
        return this.range.min;
    }

    set min(value) {
        return this.range.min = value;
    }

    get max() {
        return this.range.max;
    }

    set max(value) {
        return this.range.max = value;
    }

    get value() {
        return this.range.value;
    }

    set value(value) {
        return this.range.value = value;
    }

    get step() {
        return this.range.step;
    }

    set step(value) {
        return this.range.step = value;
    }

    get disabled() {
        return this.range.disabled;
    }

    set disabled(value) {
        return this.range.disabled = value;
    }
}

class SetupWindow {
    constructor() {
        this.inner = document.createElement("form");

        const titleHeading = document.createElement("h1");
        titleHeading.style.marginTop = "var(--pico-typography-spacing-vertical)";
        titleHeading.innerHTML = `<img src="icon.png" style="margin-right: 5px; display: inline-block; width: 60px; vertical-align: -15px;"> Lux Client`;
        this.inner.appendChild(titleHeading);

        this.addressInput = document.createElement("input");
        this.addressInput.type = "text";
        this.addressInput.placeholder = "Address";
        if (!url.searchParams.has("address")) {
            this.addressInput.value = localStorage.getItem("address");
        } else {
            this.addressInput.value = url.searchParams.get("address");
            this.addressInput.disabled = true;
        }
        this.inner.appendChild(this.addressInput);

        if (!url.searchParams.has("key")) {
            this.passwordInput = document.createElement("input");
            this.passwordInput.type = "password";
            this.passwordInput.autocomplete = "current-password";
            this.passwordInput.placeholder = "Password";
            this.passwordInput.value = localStorage.getItem("password");
            this.inner.appendChild(this.passwordInput);
        }

        if (url.searchParams.get("view_only") !== "true") {
            this.clientSideMouseCheckbox = new Checkbox("Client-side mouse");
            this.clientSideMouseCheckbox.checked = localStorage.getItem("client_side_mouse") === "true";
            this.inner.appendChild(this.clientSideMouseCheckbox.inner);

            this.simulateTouchpadCheckbox = new Checkbox("Simulate touchpad");
            this.simulateTouchpadCheckbox.checked = navigator.maxTouchPoints && localStorage.getItem("simulate_touchpad") === "true";
            this.simulateTouchpadCheckbox.disabled = !navigator.maxTouchPoints;
            this.inner.appendChild(this.simulateTouchpadCheckbox.inner);

            this.naturalTouchScrollingCheckbox = new Checkbox("Natural touch scrolling");
            this.naturalTouchScrollingCheckbox.checked = navigator.maxTouchPoints && localStorage.getItem("natural_touch_scrolling") === "true";
            this.naturalTouchScrollingCheckbox.disabled = !navigator.maxTouchPoints;
            this.inner.appendChild(this.naturalTouchScrollingCheckbox.inner);

            this.viewOnlyCheckbox = new Checkbox("View only");
            this.viewOnlyCheckbox.checked = localStorage.getItem("view_only") === "true";
            this.inner.appendChild(this.viewOnlyCheckbox.inner);
        }

        this.tcpConnectivityCheckbox = new Checkbox("TCP connectivity");
        this.tcpConnectivityCheckbox.checked = localStorage.getItem("tcp_connectivity") === "true";
        this.inner.appendChild(this.tcpConnectivityCheckbox.inner);

        this.lowPowerModeCheckbox = new Checkbox("Low power mode");
        this.lowPowerModeCheckbox.checked = localStorage.getItem("low_power_mode") === "true";
        this.inner.appendChild(this.lowPowerModeCheckbox.inner);

        if (url.searchParams.get("view_only") !== "true") {
            this.mouseSensitivityRange = new Range("Mouse sensitivity:", 0.1, 2.9, 1.5, 0.1);
            this.mouseSensitivityRange.value = parseFloat(localStorage.getItem("sensitivity"));
            this.inner.appendChild(this.mouseSensitivityRange.inner);
        }

        const submitButton = document.createElement("button");
        submitButton.type = "submit";
        submitButton.innerText = "Login";
        this.inner.appendChild(submitButton);

        const windowSizeLabel = document.createElement("p");
        windowSizeLabel.innerText = `${window.innerWidth}x${window.innerHeight}`;
        this.inner.appendChild(windowSizeLabel);

        this.inner.addEventListener("submit", this.handleSubmit.bind(this));

        this.inner.style.width = "100%";
        this.inner.style.minHeight = "100%";
        this.inner.style.paddingLeft = "15%";
        this.inner.style.paddingRight = "15%";
        this.inner.style.display = "flex";
        this.inner.style.flexDirection = "column";
        this.inner.style.justifyContent = "center";
    }

    handleSubmit(event) {
        event.preventDefault();

        if (!url.searchParams.has("key")) {
            localStorage.setItem("address", this.addressInput.value);
            localStorage.setItem("password", this.passwordInput.value);
        }
        if (url.searchParams.get("view_only") !== "true") {
            localStorage.setItem("client_side_mouse", this.clientSideMouseCheckbox.checked.toString());
            localStorage.setItem("simulate_touchpad", this.simulateTouchpadCheckbox.checked.toString());
            localStorage.setItem("natural_touch_scrolling", this.naturalTouchScrollingCheckbox.checked.toString());
            localStorage.setItem("view_only", this.viewOnlyCheckbox.checked.toString());
            localStorage.setItem("sensitivity", this.mouseSensitivityRange.value.toString());
        }
        localStorage.setItem("tcp_connectivity", this.tcpConnectivityCheckbox.checked.toString());
        localStorage.setItem("low_power_mode", this.lowPowerModeCheckbox.checked.toString());

        const videoWindow = new VideoWindow(
            this.clientSideMouseCheckbox?.checked,
            this.simulateTouchpadCheckbox?.checked,
            this.naturalTouchScrollingCheckbox?.checked,
            url.searchParams.get("view_only") === "true" || this.viewOnlyCheckbox.checked,
            this.mouseSensitivityRange?.value,
        );
        videoWindow.startStreaming(
            this.addressInput.value,
            this.passwordInput?.value,
            this.tcpConnectivityCheckbox.checked,
            this.lowPowerModeCheckbox.checked,
        );
        this.inner.replaceWith(videoWindow.inner);
    }
}

class VideoWindow {
    constructor(
        clientSideMouse = false,
        simulateTouchpad = false,
        naturalTouchScrolling = false,
        viewOnly = false,
        mouseSensitivity = 1.5,
    ) {
        this.inner = document.createElement("div");

        this.clientSideMouse = clientSideMouse;
        this.simulateTouchpad = simulateTouchpad;
        this.naturalTouchScrolling = naturalTouchScrolling;
        this.viewOnly = viewOnly;
        this.mouseSensitivity = mouseSensitivity;

        this.virtualMouseX = window.innerWidth / 2;
        this.virtualMouseY = window.innerHeight / 2;

        this.touches = [];
        this.lastRightClickTime = 0;

        this.currentPenStroke = [];

        this.cachedVideoWidth = 1;
        this.cachedVideoHeight = 1;
        this.cachedCanvasWidth = 1;
        this.cachedCanvasHeight = 1;

        this.inner.style.width = "100%";
        this.inner.style.height = "100%";
        this.inner.style.position = "relative";
        this.inner.style.display = "flex";
        this.inner.style.justifyContent = "center";
        this.inner.style.alignItems = "center";
    }

    listen(target, type, listener, options = {}) {
        target.addEventListener(type, listener.bind(this), options);
    }

    attachEventListeners() {
        this.listen(this.video, "resize", () => {
            this.cachedVideoWidth = this.video.videoWidth || 1;
            this.cachedVideoHeight = this.video.videoHeight || 1;
        });
        this.listen(window, "resize", this.handleResize);

        if (this.viewOnly) return;

        this.listen(document, "contextmenu", event => event.preventDefault());

        if (!this.clientSideMouse) {
            this.listen(this.canvas, "click", this.handleCanvasClick);
        } else if (this.simulateTouchpad) {
            this.mouseImage = new Image();
            this.listen(this.mouseImage, "load", this.draw, { once: true });
            this.mouseImage.src = "mouse.png";
        }

        this.listen(this.canvas, "mousemove", this.handleMouseMove);
        this.listen(this.canvas, "mousedown", this.handleMouseDown);
        this.listen(this.canvas, "mouseup", this.handleMouseUp);
        this.listen(document, "wheel", this.handleWheel, { passive: false });
        this.listen(document, "keydown", this.handleKeyDown);
        this.listen(document, "keyup", this.handleKeyUp);
        this.listen(this.canvas, "touchstart", event => event.preventDefault());
        this.listen(this.canvas, "touchend", event => event.preventDefault());
        this.listen(this.canvas, "touchmove", event => event.preventDefault());
        this.listen(this.canvas, "pointerdown", this.handlePointerDown);
        this.listen(this.canvas, "pointerup", this.handlePointerUp);
        this.listen(this.canvas, "pointercancel", this.handlePointerCancel);
        this.listen(this.canvas, "pointermove", this.handlePointerMove);
    }

    async startStreaming(address, password, askCamera, lowPowerMode = false) {
        this.inner.ariaBusy = true;
        this.inner.innerText = "Connecting...";

        if (askCamera) {
            try {
                const stream = await navigator.mediaDevices.getUserMedia({
                    audio: false,
                    video: true,
                });
                stream.getTracks().forEach(track => track.stop());
            } catch (e) {
                this.inner.innerText += " (without TCP support, due to insufficient permissions)";
            }
        }

        const conn = new RTCPeerConnection({
            iceServers: [
                {
                    urls: "stun:stun.l.google.com:19302",
                },
            ],
        });

        this.listen(conn, "iceconnectionstatechange", () => {
            if (conn.iceConnectionState === "closed" ||
                conn.iceConnectionState === "failed") {
                if (!url.searchParams.has("key")) {
                    url.searchParams.set("reconnect", "true");
                    window.location.href = url.toString();
                } else {
                    alert("The connection has closed.");
                    window.location.href = window.location.origin + window.location.pathname;
                }
            }
        });

        if (!this.viewOnly) {
            this.orderedChannel = conn.createDataChannel("ordered-input", {
                ordered: true,
            });
            this.unorderedChannel = conn.createDataChannel("unordered-input", {
                ordered: false,
            });
        }

        this.listen(conn, "track", event => {
            const media = document.createElement(event.track.kind);
            if (event.track.kind === "video") {
                this.video = media;
                this.video.controls = false;
                this.video.playsInline = true;
                this.video.muted = true;
                this.video.autoplay = true;
                this.video.srcObject = new MediaStream([event.track]);
                this.video.play()?.catch(e => console.error("Video playback failed:", e));

                if (!this.viewOnly) {
                    this.canvas = document.createElement("canvas");
                    this.ctx = this.canvas.getContext("2d");
                }

                this.video.style.minWidth = "0";
                this.video.style.flex = "1";
                this.video.style.objectFit = "contain";
                this.video.style.userSelect = "none";
                this.video.style.webkitUserSelect = "none";

                if (!this.viewOnly) {
                    this.canvas.style.position = "absolute";
                    this.canvas.style.top = "0";
                    this.canvas.style.left = "0";
                    this.canvas.style.width = "100%";
                    this.canvas.style.height = "100%";
                    this.canvas.style.touchAction = "none";
                    this.canvas.style.userSelect = "none";
                    this.canvas.style.webkitUserSelect = "none";
                    this.canvas.style.webkitTouchCallout = "none";
                }

                this.attachEventListeners();

                this.inner.innerText = "";
                this.inner.ariaBusy = false;
                this.inner.style.removeProperty("justify-content");
                this.inner.style.removeProperty("align-items");

                this.inner.appendChild(this.video);
                if (!this.viewOnly) this.inner.appendChild(this.canvas);
                this.handleResize();
            } else if (event.track.kind === "audio") {
                this.audio = media;
                this.audio.controls = false;
                this.audio.autoplay = true;
                this.audio.srcObject = new MediaStream([event.track]);
                this.audio.play()?.catch(e => console.error("Audio playback failed:", e));
            }
        });

        this.listen(conn, "icecandidate", async event => {
            if (!event.candidate) {
                let resp;
                try {
                    resp = await fetch(`https://${address}/offer`, {
                        method: "POST",
                        headers: {
                            "Content-Type": "application/json",
                        },
                        body: JSON.stringify(!url.searchParams.has("key") ? {
                            password,
                            show_mouse: !this.clientSideMouse || this.viewOnly,
                            low_power_mode: lowPowerMode,
                            offer: btoa(JSON.stringify(conn.localDescription)),
                        } : {
                            key: url.searchParams.get("key"),
                            show_mouse: !this.clientSideMouse || this.viewOnly,
                            low_power_mode: lowPowerMode,
                            offer: btoa(JSON.stringify(conn.localDescription)),
                        }),
                    });
                } catch (e) {
                    alert(`Error: ${e}`);
                    window.location.href = window.location.origin + window.location.pathname;
                    return;
                }

                if (resp.status === 200) {
                    try {
                        const answer = await resp.text();
                        const desc = JSON.parse(atob(JSON.parse(answer).Offer));
                        console.log("Remote description:", desc);
                        desc.sdp = desc.sdp.replace(/typ srflx tcptype (passive|active)/g, "typ host tcptype $1"); // Firefox is garbage!
                        desc.sdp = desc.sdp.replace("useinbandfec=1", "useinbandfec=0;stereo=1");
                        await conn.setRemoteDescription(new RTCSessionDescription(desc));
                    } catch (e) {
                        alert(`Error: ${e}`);
                        window.location.href = window.location.origin + window.location.pathname;
                    }
                } else {
                    let message;
                    try {
                        message = (await resp.json()).Error;
                    } catch (e) {
                        message = `HTTP ${resp.status}`;
                    }
                    message ||= `HTTP ${resp.status}`;
                    alert(`Error: ${message}`);
                    window.location.href = window.location.origin + window.location.pathname;
                }
            }
        });

        // Offer to receive a video track and an audio track
        conn.addTransceiver("video", { direction: "recvonly" });
        conn.addTransceiver("audio", { direction: "recvonly" });
        try {
            const offer = await conn.createOffer();
            const desc = {
                type: offer.type,
                sdp: offer.sdp.replace("useinbandfec=1", "useinbandfec=0;stereo=1"),
            };
            console.log("Local description:", desc);
            await conn.setLocalDescription(new RTCSessionDescription(desc));
        } catch (e) {
            alert(`Error: ${e}`);
            window.location.href = window.location.origin + window.location.pathname;
        }
    }

    sendOrdered(message) {
        if (this.orderedChannel.readyState === "open") {
            this.orderedChannel.send(JSON.stringify(message));
        }
    }

    sendUnordered(message) {
        if (this.unorderedChannel.readyState === "open") {
            this.unorderedChannel.send(JSON.stringify(message));
        }
    }

    async handleCanvasClick() {
        if (this.canvas.requestPointerLock) {
            try {
                await this.canvas.requestPointerLock({
                    unadjustedMovement: true,
                });
            } catch (e) {
                this.canvas.requestPointerLock();
            }
        }
    }

    positionInVideo(x, y) {
        const vw = this.cachedVideoWidth;
        const vh = this.cachedVideoHeight;
        const cw = this.cachedCanvasWidth;
        const ch = this.cachedCanvasHeight;

        if (vw * ch > cw * vh) {
            return {
                x: Math.round(x * vw / cw),
                y: Math.round((y - ch / 2) * vw / cw + vh / 2),
            };
        } else {
            return {
                x: Math.round((x - cw / 2) * vh / ch + vw / 2),
                y: Math.round(y * vh / ch),
            };
        }
    }

    moveVirtualMouse(x, y) {
        this.virtualMouseX = Math.min(Math.max(this.virtualMouseX + x, 0), this.cachedCanvasWidth - 1);
        this.virtualMouseY = Math.min(Math.max(this.virtualMouseY + y, 0), this.cachedCanvasHeight - 1);
        this.draw();
    }

    draw() {
        this.ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);

        // Draw pen stroke
        if (this.currentPenStroke.length >= 2) {
            this.ctx.save();
            this.ctx.scale(window.devicePixelRatio, window.devicePixelRatio);
            this.ctx.strokeStyle = "rgba(0, 0, 0, 0.35)";
            this.ctx.lineWidth = 2;
            this.ctx.lineCap = "round";
            this.ctx.lineJoin = "round";
            this.ctx.beginPath();
            this.ctx.moveTo(this.currentPenStroke[0].x, this.currentPenStroke[0].y);
            for (let i = 1; i < this.currentPenStroke.length; ++i) {
                this.ctx.lineTo(this.currentPenStroke[i].x, this.currentPenStroke[i].y);
            }
            this.ctx.stroke();
            this.ctx.restore();
        }

        // Draw mouse
        if (this.clientSideMouse && this.simulateTouchpad && this.mouseImage.complete) {
            this.ctx.save();
            this.ctx.shadowColor = "rgba(0, 0, 0, 0.3)";
            this.ctx.shadowBlur = 2.5 * window.devicePixelRatio;
            this.ctx.shadowOffsetX = 1.5 * window.devicePixelRatio;
            this.ctx.shadowOffsetY = 1.5 * window.devicePixelRatio;
            this.ctx.drawImage(
                this.mouseImage,
                Math.round(this.virtualMouseX * window.devicePixelRatio),
                Math.round(this.virtualMouseY * window.devicePixelRatio),
                Math.round(this.mouseImage.width / 40 * window.devicePixelRatio),
                Math.round(this.mouseImage.height / 40 * window.devicePixelRatio),
            );
            this.ctx.restore();
        }
    }

    pushTouch(touch) {
        this.touches.push({
            id: touch.id,
            clientX: touch.clientX,
            clientY: touch.clientY,
            initialClientX: touch.clientX,
            initialClientY: touch.clientY,
            startTime: Date.now(),
        });
    }

    clearTouches(mouseup = true) {
        if (this.simulateTouchpad) {
            if (mouseup) {
                const message = {
                    type: "mouseup",
                };
                message.button = 0;
                this.sendOrdered(message);
                message.button = 2;
                this.sendOrdered(message);
            }
            this.touches = [];
        } else {
            for (const touch of this.touches) {
                const message = {
                    type: "touchend",
                    id: Math.abs(touch.id) % 10,
                };
                this.sendOrdered(message);
            }
            this.touches = [];
        }
    }

    handleMouseMove(event) {
        if (this.clientSideMouse) {
            const message = {
                type: "mousemoveabs",
                ...this.positionInVideo(event.clientX, event.clientY),
            };
            this.sendOrdered(message);
        } else {
            const message = {
                type: "mousemove",
                x: Math.round(event.movementX),
                y: Math.round(event.movementY),
            };
            this.sendUnordered(message);
        }
    }

    handleMouseDown(event) {
        const message = {
            type: "mousedown",
            button: event.button,
        };
        this.sendOrdered(message);
    }

    handleMouseUp(event) {
        const message = {
            type: "mouseup",
            button: event.button,
        };
        this.sendOrdered(message);
    }

    handleWheel(event) {
        event.preventDefault();

        const message = {
            type: "wheel",
            x: Math.round(event.deltaX),
            y: Math.round(event.deltaY),
        };
        this.sendUnordered(message);
    }

    handleKeyDown(event) {
        event.preventDefault();

        const message = {
            type: "keydown",
            key: event.code,
        };
        this.sendOrdered(message);
    }

    handleKeyUp(event) {
        event.preventDefault();

        const message = {
            type: "keyup",
            key: event.code,
        };
        this.sendOrdered(message);
    }

    handleTouchStart(newTouches) {
        if (this.simulateTouchpad) {
            for (const touch of newTouches) {
                if (touch.radiusX <= 60 && touch.radiusY <= 60) {
                    this.pushTouch(touch);
                }
            }

            switch (this.touches.length) {
                case 3: {
                    if (this.clientSideMouse) {
                        const message = {
                            type: "mousemoveabs",
                            ...this.positionInVideo(this.virtualMouseX, this.virtualMouseY),
                        };
                        this.sendOrdered(message);
                    }

                    // Start drag
                    const message = {
                        type: "mousedown",
                        button: 0,
                    };
                    this.sendOrdered(message);
                    break;
                }
            }
        } else {
            for (const touch of newTouches) {
                if (touch.radiusX <= 60 && touch.radiusY <= 60) {
                    this.pushTouch(touch);

                    const message = {
                        type: "touchstart",
                        id: Math.abs(touch.id) % 10,
                        ...this.positionInVideo(touch.clientX, touch.clientY),
                    };
                    this.sendOrdered(message);
                }
            }
        }
    }

    async handleTouchEnd(deletedTouches) {
        deletedTouches = deletedTouches.filter(deletedTouch => this.touches.some(touch => touch.id === deletedTouch.id));
        if (!deletedTouches.length) return;

        if (this.simulateTouchpad) {
            switch (this.touches.length) {
                case 1: {
                    const now = Date.now();
                    if (now - this.lastRightClickTime > 125 && now - this.touches[0].startTime <= 125) {
                        if (this.clientSideMouse) {
                            const message = {
                                type: "mousemoveabs",
                                ...this.positionInVideo(this.virtualMouseX, this.virtualMouseY),
                            };
                            this.sendOrdered(message);
                        }

                        const message = {
                            button: 0,
                        };
                        message.type = "mousedown";
                        this.sendOrdered(message);
                        message.type = "mouseup";
                        this.sendOrdered(message);

                        // Make lone touches linger to improve two-finger tap detection
                        await sleep(125);
                    }
                    break;
                }

                case 2: {
                    const now = Date.now();
                    if (this.touches.every(touch => now - touch.startTime <= 250) &&
                        this.touches.every(
                            touch => distance(
                                touch.clientX,
                                touch.clientY,
                                touch.initialClientX,
                                touch.initialClientY,
                            ) <= 25
                        ) &&
                        distance(
                            this.touches[0].clientX,
                            this.touches[0].clientY,
                            this.touches[1].clientX,
                            this.touches[1].clientY,
                        ) >= 20) {
                        if (this.clientSideMouse) {
                            const message = {
                                type: "mousemoveabs",
                                ...this.positionInVideo(this.virtualMouseX, this.virtualMouseY),
                            };
                            this.sendOrdered(message);
                        }

                        const message = {
                            button: 2,
                        };
                        message.type = "mousedown";
                        this.sendOrdered(message);
                        message.type = "mouseup";
                        this.sendOrdered(message);

                        this.lastRightClickTime = now;
                    }
                    break;
                }

                case 3: {
                    // End drag
                    const message = {
                        type: "mouseup",
                        button: 0,
                    };
                    this.sendOrdered(message);
                    break;
                }
            }
        } else {
            for (const deletedTouch of deletedTouches) {
                let touch;
                if (touch = this.touches.find(touch => touch.id === deletedTouch.id)) {
                    const message = {
                        type: "touchend",
                        id: Math.abs(touch.id) % 10,
                    };
                    this.sendOrdered(message);
                }
            }
        }

        this.touches = this.touches.filter(touch => !deletedTouches.some(deletedTouch => deletedTouch.id === touch.id));
    }

    handleTouchMove(movedTouches) {
        movedTouches = movedTouches.filter(movedTouch => this.touches.some(touch => touch.id === movedTouch.id));
        if (!movedTouches.length) return;

        if (this.simulateTouchpad) {
            switch (this.touches.length) {
                case 1: {
                    if (this.clientSideMouse) {
                        this.moveVirtualMouse(
                            (movedTouches[0].clientX - this.touches[0].clientX) * this.mouseSensitivity,
                            (movedTouches[0].clientY - this.touches[0].clientY) * this.mouseSensitivity,
                        );
                        const message = {
                            type: "mousemoveabs",
                            ...this.positionInVideo(this.virtualMouseX, this.virtualMouseY),
                        };
                        this.sendOrdered(message);
                    } else {
                        const message = {
                            type: "mousemove",
                            x: Math.round((movedTouches[0].clientX - this.touches[0].clientX) * this.mouseSensitivity),
                            y: Math.round((movedTouches[0].clientY - this.touches[0].clientY) * this.mouseSensitivity),
                        };
                        this.sendUnordered(message);
                    }
                    break;
                }

                case 2: {
                    const now = Date.now();
                    if (movedTouches[0].id === this.touches[0].id && this.touches.every(touch => now - touch.startTime >= 25)) {
                        const message = {
                            type: "wheel",
                            x: Math.round((movedTouches[0].clientX - this.touches[0].clientX) * (this.naturalTouchScrolling ? -1 : 1) * 6),
                            y: Math.round((movedTouches[0].clientY - this.touches[0].clientY) * (this.naturalTouchScrolling ? -1 : 1) * 6),
                        };
                        this.sendUnordered(message);
                    }
                    break;
                }

                case 3: {
                    if (movedTouches[0].id === this.touches[0].id) {
                        if (this.clientSideMouse) {
                            this.moveVirtualMouse(
                                (movedTouches[0].clientX - this.touches[0].clientX) * this.mouseSensitivity,
                                (movedTouches[0].clientY - this.touches[0].clientY) * this.mouseSensitivity,
                            );
                            const message = {
                                type: "mousemoveabs",
                                ...this.positionInVideo(this.virtualMouseX, this.virtualMouseY),
                            };
                            this.sendOrdered(message);
                        } else {
                            const message = {
                                type: "mousemove",
                                x: Math.round((movedTouches[0].clientX - this.touches[0].clientX) * this.mouseSensitivity),
                                y: Math.round((movedTouches[0].clientY - this.touches[0].clientY) * this.mouseSensitivity),
                            };
                            this.sendUnordered(message);
                        }
                    }
                    break;
                }
            }
        } else {
            for (const movedTouch of movedTouches) {
                const message = {
                    type: "touchmove",
                    id: Math.abs(movedTouch.id) % 10,
                    ...this.positionInVideo(movedTouch.clientX, movedTouch.clientY),
                };
                this.sendOrdered(message);
            }
        }

        for (const touch of this.touches) {
            for (const movedTouch of movedTouches) {
                if (touch.id === movedTouch.id) {
                    touch.clientX = movedTouch.clientX;
                    touch.clientY = movedTouch.clientY;
                }
            }
        }
    }

    handlePointerDown(event) {
        if (event.pointerType === "touch") {
            this.handleTouchStart([{
                id: event.pointerId,
                clientX: event.clientX,
                clientY: event.clientY,
                radiusX: event.width / 2,
                radiusY: event.height / 2,
            }]);
        } else if (event.pointerType === "pen") {
            this.clearTouches();

            // Pen input on iOS is ASTOUNDINGLY BROKEN!
            // Safari gives you TWO of every pen-related event, so they must be deduplicated
            // MANY SUCH CASES - see comments in Tenebra's input code
            const message = {
                type: "pen",
                ...this.positionInVideo(event.clientX, event.clientY),
                pressure: Math.max(event.pressure, 0.001),
                tiltX: Math.round(event.tiltX),
                tiltY: Math.round(event.tiltY),
            };
            if (!shallowEqual(message, this.lastPenMessage)) {
                // Google Chrome is ALSO ASTOUNDINGLY BROKEN!
                // Chrome often fires pointermove events BEFORE the corresponding pointerdown
                // As a result, we must check if this.currentPenStroke is already populated before overwriting it
                if (!this.currentPenStroke.length) {
                    this.currentPenStroke = [{ x: event.clientX, y: event.clientY, time: Date.now() }];
                }

                this.sendOrdered(message);
                this.lastPenMessage = message;
            }
        }
    }

    handlePointerUp(event) {
        if (event.pointerType === "touch") {
            this.handleTouchEnd([{
                id: event.pointerId,
                clientX: event.clientX,
                clientY: event.clientY,
                radiusX: event.width / 2,
                radiusY: event.height / 2,
            }]);
        } else if (event.pointerType === "pen") {
            const message = {
                type: "pen",
                ...this.positionInVideo(event.clientX, event.clientY),
                pressure: 0,
                tiltX: Math.round(event.tiltX),
                tiltY: Math.round(event.tiltY),
            };
            if (!shallowEqual(message, this.lastPenMessage)) {
                this.currentPenStroke = [];
                this.draw();

                this.sendOrdered(message);
                this.lastPenMessage = message;
            }
        }
    }

    handlePointerCancel(event) {
        if (event.pointerType === "pen") {
            this.handlePointerUp(event);
            return;
        }
        if (event.pointerType !== "touch") return;

        const touch = this.touches.find(touch => touch.id === event.pointerId);
        if (!touch) return;

        if (this.simulateTouchpad) {
            if (this.touches.length === 3) {
                this.sendOrdered({
                    type: "mouseup",
                    button: 0,
                });
            }
        } else {
            this.sendOrdered({
                type: "touchend",
                id: Math.abs(touch.id) % 10,
            });
        }

        this.touches = this.touches.filter(touch => touch.id !== event.pointerId);
    }

    handlePointerMove(event) {
        if (event.pointerType === "touch") {
            this.handleTouchMove([{
                id: event.pointerId,
                clientX: event.clientX,
                clientY: event.clientY,
                radiusX: event.width / 2,
                radiusY: event.height / 2,
            }]);
        } else if (event.pointerType === "pen") {
            this.clearTouches(false);

            const message = {
                type: "pen",
                ...this.positionInVideo(event.clientX, event.clientY),
                pressure: Math.max(event.pressure, 0.001),
                tiltX: Math.round(event.tiltX),
                tiltY: Math.round(event.tiltY),
            };
            if (!shallowEqual(message, this.lastPenMessage)) {
                const now = Date.now();
                if (event.getCoalescedEvents) {
                    for (const coalescedEvent of event.getCoalescedEvents()) {
                        // Since Chrome is broken, we must ensure that this.currentPenStroke is populated before removing old points
                        if (this.currentPenStroke.length && now - this.currentPenStroke[0].time > 1000 / 60 * 20) {
                            this.currentPenStroke.shift();
                        }
                        this.currentPenStroke.push({ x: coalescedEvent.clientX, y: coalescedEvent.clientY, time: now });

                        const coalescedMessage = {
                            type: "pen",
                            ...this.positionInVideo(coalescedEvent.clientX, coalescedEvent.clientY),
                            pressure: Math.max(coalescedEvent.pressure, 0.001),
                            tiltX: Math.round(coalescedEvent.tiltX),
                            tiltY: Math.round(coalescedEvent.tiltY),
                        };
                        this.sendOrdered(coalescedMessage);
                    }
                } else {
                    if (this.currentPenStroke.length && now - this.currentPenStroke[0].time > 1000 / 60 * 20) {
                        this.currentPenStroke.shift();
                    }
                    this.currentPenStroke.push({ x: event.clientX, y: event.clientY, time: now });

                    this.sendOrdered(message);
                }
                this.draw();

                this.lastPenMessage = message;
            }
        }
    }

    handleResize() {
        if (this.canvas) {
            this.canvas.width = this.canvas.clientWidth * window.devicePixelRatio;
            this.canvas.height = this.canvas.clientHeight * window.devicePixelRatio;

            this.cachedCanvasWidth = this.canvas.clientWidth || 1;
            this.cachedCanvasHeight = this.canvas.clientHeight || 1;
        }
        if (this.video) {
            this.cachedVideoWidth = this.video.videoWidth || 1;
            this.cachedVideoHeight = this.video.videoHeight || 1;
        }

        if (!this.viewOnly) {
            if (this.clientSideMouse && this.simulateTouchpad) {
                this.virtualMouseX = Math.min(this.virtualMouseX, this.cachedCanvasWidth - 1);
                this.virtualMouseY = Math.min(this.virtualMouseY, this.cachedCanvasHeight - 1);
            }
            this.draw();
        }
    }
}

if (url.searchParams.get("reconnect") === "true") {
    const videoWindow = new VideoWindow(
        localStorage.getItem("client_side_mouse") === "true",
        localStorage.getItem("simulate_touchpad") === "true",
        localStorage.getItem("natural_touch_scrolling") === "true",
        localStorage.getItem("view_only") === "true",
        parseFloat(localStorage.getItem("sensitivity")),
    );
    videoWindow.startStreaming(
        localStorage.getItem("address"),
        localStorage.getItem("password"),
        localStorage.getItem("tcp_connectivity") === "true",
        localStorage.getItem("low_power_mode") === "true",
    );
    document.body.appendChild(videoWindow.inner);
} else {
    const setupWindow = new SetupWindow();
    document.body.appendChild(setupWindow.inner);
}
