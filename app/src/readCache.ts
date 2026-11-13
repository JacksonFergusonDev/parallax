interface Entry {
  promise: Promise<unknown>;
  expires: number;
  tags: readonly string[];
  /** Set once the read succeeds, for `peek`. */
  settled?: { value: unknown };
}

/** Bounded, session-only cache for reads. Shares pending requests, never caches
 * failures, and forgets invalidated requests even if they finish afterward. */
export class ReadCache {
  private entries = new Map<string, Entry>();

  constructor(private readonly capacity = 128) {}

  get<T>(
    key: readonly unknown[],
    load: () => Promise<T>,
    tags: readonly string[],
    maxAge = 5 * 60_000,
  ): Promise<T> {
    const id = JSON.stringify(key);
    const existing = this.entries.get(id);
    if (existing && existing.expires > Date.now()) {
      // Insertion order tracks least-recently-used entries.
      this.entries.delete(id);
      this.entries.set(id, existing);
      return existing.promise as Promise<T>;
    }
    const entry: Entry = {
      promise: Promise.resolve().then(load),
      expires: Infinity,
      tags,
    };
    entry.promise = entry.promise.then(
      (value) => {
        entry.expires = Date.now() + maxAge;
        entry.settled = { value };
        return value;
      },
      (error: unknown) => {
        if (this.entries.get(id) === entry) this.entries.delete(id);
        throw error;
      },
    );
    this.entries.delete(id);
    this.entries.set(id, entry);
    while (this.entries.size > this.capacity) {
      this.entries.delete(this.entries.keys().next().value!);
    }
    return entry.promise as Promise<T>;
  }

  /** A finished, unexpired read's value right away, so a view can draw
   * loaded data on its first render instead of a loading state. */
  peek<T>(key: readonly unknown[]): T | undefined {
    const entry = this.entries.get(JSON.stringify(key));
    if (!entry?.settled || entry.expires <= Date.now()) return undefined;
    return entry.settled.value as T;
  }

  /** Stores a value read some other way, replacing any entry for `key`. */
  put<T>(
    key: readonly unknown[],
    value: T,
    tags: readonly string[],
    maxAge = 5 * 60_000,
  ) {
    const id = JSON.stringify(key);
    this.entries.delete(id);
    this.entries.set(id, {
      promise: Promise.resolve(value),
      expires: Date.now() + maxAge,
      tags,
      settled: { value },
    });
    while (this.entries.size > this.capacity) {
      this.entries.delete(this.entries.keys().next().value!);
    }
  }

  invalidate(...tags: string[]) {
    for (const [key, entry] of this.entries) {
      if (entry.tags.some((tag) => tags.includes(tag)))
        this.entries.delete(key);
    }
  }

  clear() {
    this.entries.clear();
  }
}

export const readCache = new ReadCache();

/** Full-size cover dimensions are expensive to fetch repeatedly in the picker. */
export function coverDimensions(url: string): Promise<[number, number]> {
  return readCache.get(
    ["image-size", url],
    () =>
      new Promise((resolve, reject) => {
        const image = new Image();
        image.onload = () => resolve([image.naturalWidth, image.naturalHeight]);
        image.onerror = () => reject(new Error("Cover image unavailable"));
        image.src = url;
      }),
    ["image-sizes"],
    60 * 60_000,
  );
}
