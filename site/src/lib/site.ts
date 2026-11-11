// Where the site links to, in one place.
export const repository = 'JacksonFergusonDev/parallax';

export const links = {
  github: `https://github.com/${repository}`,
  releases: `https://github.com/${repository}/releases`,
  license: `https://github.com/${repository}/blob/main/LICENSE`,
  deviceSupport: `https://github.com/${repository}/blob/main/docs/device-support.md`,
  issues: `https://github.com/${repository}/issues/new/choose`,
  linkedin: 'https://www.linkedin.com/in/jackson--ferguson/',
  email: 'mailto:jackson.ferguson0@gmail.com',
  download: '/download/',
} as const;

// Whether releases are notarized by Apple. Until they are, macOS blocks the
// first open and the download page explains how to allow it. Set this to true
// once the release workflow has the Developer ID secrets
// (engineering/releasing.md).
export const notarized = false;
