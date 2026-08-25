import { PosShell } from "@/components/pos-shell";
import { SiteHeader } from "@/components/site-header";

export default function PosPage() {
  return (
    <>
      <SiteHeader active="pos" />
      <PosShell />
    </>
  );
}
