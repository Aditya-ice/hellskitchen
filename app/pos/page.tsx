import { AuthGate } from "@/components/auth-gate";
import { PosSurface } from "@/components/pos-surface";

export default function PosPage() {
  return (
    <AuthGate>
      <PosSurface />
    </AuthGate>
  );
}
