const SESSION_PATTERN = /^[0-9a-f-]{36}$/i;
const validTokens = new Set();
const rateLimitBuckets = new Map();
export function issueSessionToken() {
    const token = crypto.randomUUID();
    validTokens.add(token);
    return token;
}
export function isValidSessionToken(token) {
    return SESSION_PATTERN.test(token) && validTokens.has(token);
}
export function extractBearerToken(authHeader) {
    if (!authHeader)
        return null;
    const match = /^Bearer\s+([0-9a-f-]+)$/i.exec(authHeader.trim());
    return match ? match[1] : null;
}
export function enforceRateLimit(key, limit, windowMs) {
    const now = Date.now();
    const timestamps = (rateLimitBuckets.get(key) ?? []).filter((time) => now - time < windowMs);
    if (timestamps.length >= limit) {
        const oldest = timestamps[0];
        const retryAfter = Math.max(1, Math.ceil((windowMs - (now - oldest)) / 1000));
        return { allowed: false, retryAfter };
    }
    timestamps.push(now);
    rateLimitBuckets.set(key, timestamps);
    return { allowed: true };
}
export const requireAuth = async (c, next) => {
    const authHeader = c.req.header("authorization");
    const token = extractBearerToken(authHeader);
    if (!token || !isValidSessionToken(token)) {
        return c.json({ error: "Valid demo session token required. Call POST /v1/session first." }, 401);
    }
    c.set("sessionToken", token);
    await next();
};
export function getClientIp(c) {
    return (c.req.header("x-forwarded-for")?.split(",")[0]?.trim() ||
        c.req.header("x-real-ip") ||
        "127.0.0.1");
}
