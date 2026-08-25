import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { PosProvider } from "@/components/pos-provider";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Ember POS | Guest Decisions, Made Clear",
  description:
    "AI-assisted seating and ordering for modern front-of-house teams.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html
      lang="en"
      className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}
    >
      <body className="min-h-full">
        <PosProvider>{children}</PosProvider>
      </body>
    </html>
  );
}
