const CACHE_NAME = "static-cache";
const ASSETS_TO_CACHE = [
    "/",
    "/index.html",
    "/pico.classless.min.css",
    "/index.js",
    "/manifest.json",
    "/favicon.ico",
    "/icon.png",
    "/apple-touch-icon.png",
    "/mouse.png",
];

self.addEventListener("install", event => {
    event.waitUntil(
        caches.open(CACHE_NAME)
            .then((cache) => cache.addAll(ASSETS_TO_CACHE))
            .then(() => self.skipWaiting())
    );
});

self.addEventListener("activate", event => {
    event.waitUntil(
        caches.keys().then(keys =>
            Promise.all(
                keys
                    .filter(key => key !== CACHE_NAME)
                    .map(key => caches.delete(key))
            )
        )
    );
    event.waitUntil(self.clients.claim());
});

self.addEventListener("fetch", (event) => {
    // Only handle HTTP and HTTPS GET requests
    if (event.request.method !== "GET" || !event.request.url.startsWith("http")) {
        return;
    }

    event.respondWith(
        fetch(event.request)
            .then((networkResponse) => {
                // Update the cache only if the response is a successful one
                if (networkResponse && (networkResponse.status === 200 || networkResponse.type === "opaque")) {
                    const responseClone = networkResponse.clone();
                    caches.open(CACHE_NAME).then((cache) => {
                        cache.put(event.request, responseClone);
                    });
                }
                return networkResponse;
            })
            .catch(() => {
                // Fallback to cached response when offline
                return caches.match(event.request);
            })
    );
});
