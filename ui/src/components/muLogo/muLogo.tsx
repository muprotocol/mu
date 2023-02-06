import Image from "next/image";

export function MuLogo() {
  return (
    <div className="flex items-baseline select-none gap-4">
      <Image
        src="/mu.svg"
        alt="mu"
        className="h-6 w-auto"
        width="64"
        height="64"
        data-testid="muLogo"
      />
      <Image
        src="/protocol.svg"
        alt="protocol"
        className="hidden h-6 w-auto md:block"
        width="12"
        height="12"
        data-testid="protocolLogo"
      />
    </div>
  );
}
