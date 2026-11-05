/** A small shelf of books, for empty states. */
export function ShelfArt() {
  const books = [
    { x: 8, w: 14, h: 58, c: "#b4532a" },
    { x: 24, w: 10, h: 50, c: "#264653" },
    { x: 36, w: 16, h: 62, c: "#e9c46a" },
    { x: 54, w: 11, h: 46, c: "#3d5a3f" },
  ];
  return (
    <svg
      className="shelf-art"
      viewBox="0 0 110 72"
      width="110"
      height="72"
      aria-hidden="true"
    >
      {books.map((b) => (
        <rect
          key={b.x}
          x={b.x}
          y={68 - b.h}
          width={b.w}
          height={b.h}
          rx="1.5"
          fill={b.c}
        />
      ))}
      <rect
        x="70"
        y="22"
        width="13"
        height="48"
        rx="1.5"
        fill="#6b2d3c"
        transform="rotate(14 70 70)"
      />
      <rect x="2" y="68" width="106" height="3" rx="1.5" fill="currentColor" />
    </svg>
  );
}
