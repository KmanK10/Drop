import { useId, useState } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function PasswordField({
  label,
  value,
  onChange,
  autoComplete,
  name,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  autoComplete: "current-password" | "new-password" | "off";
  name: string;
}) {
  const id = useId();
  const [show, setShow] = useState(false);
  return (
    <div className="grid gap-2">
      <div className="flex items-center justify-between gap-3">
        <Label htmlFor={id}>{label}</Label>
        <button
          type="button"
          className="min-h-11 shrink-0 px-1 text-sm text-muted-foreground underline-offset-4 hover:underline"
          onClick={() => setShow((current) => !current)}
        >
          {show ? "Hide" : "Show"}
        </button>
      </div>
      <Input
        id={id}
        name={name}
        type={show ? "text" : "password"}
        autoComplete={autoComplete}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        required
      />
    </div>
  );
}
