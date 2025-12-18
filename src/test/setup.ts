import { randomFillSync } from "crypto";
import { beforeAll, afterEach, vi } from "vitest";

// Polyfill WebCrypto for jsdom (required for Tauri mocks)
beforeAll(() => {
  Object.defineProperty(window, 'crypto', {
    value: {
      getRandomValues: (buffer: Uint8Array) => randomFillSync(buffer)
    }
  });
});

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

afterEach(() => {
  vi.clearAllMocks();
});
