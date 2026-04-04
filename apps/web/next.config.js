/** @type {import('next').NextConfig} */
const imageDomains = ['localhost'];
// Allow LAN IP for image loading when running on the network
if (process.env.LAN_HOST) {
  imageDomains.push(process.env.LAN_HOST);
}
const nextConfig = {
  output: 'standalone',
  reactStrictMode: true,
  images: {
    domains: imageDomains,
  },
  async rewrites() {
    return [
      {
        source: '/api/v1/:path*',
        destination: `${process.env.NEXT_PUBLIC_API_URL || 'http://localhost:8080'}/api/v1/:path*`,
      },
    ];
  },
};

module.exports = nextConfig;
