import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { Check, ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type ComboActionOption = {
  id: string;
  label: string;
  disabled?: boolean;
};

type ComboActionButtonProps = {
  options: ComboActionOption[];
  value: string;
  onValueChange: (id: string) => void;
  onRun: () => void;
  disabled?: boolean;
  loading?: boolean;
  loadingLabel?: string;
  menuAriaLabel: string;
  runAriaLabel: string;
};

export function ComboActionButton({
  options,
  value,
  onValueChange,
  onRun,
  disabled,
  loading,
  loadingLabel = "…",
  menuAriaLabel,
  runAriaLabel,
}: ComboActionButtonProps) {
  const selected =
    options.find((o) => o.id === value) ?? options.find((o) => !o.disabled);
  const canMenu = options.length > 1;
  const runDisabled = disabled || loading || selected?.disabled || !selected;

  return (
    <div className="inline-flex rounded-md">
      <Button
        type="button"
        variant="secondary"
        size="sm"
        disabled={runDisabled}
        aria-label={runAriaLabel}
        className={cn(canMenu && "rounded-r-none border-r border-border")}
        onClick={onRun}
      >
        {loading ? loadingLabel : (selected?.label ?? "")}
      </Button>
      {canMenu ? (
        <DropdownMenu.Root>
          <DropdownMenu.Trigger asChild>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={disabled || loading}
              className="rounded-l-none px-2"
              aria-label={menuAriaLabel}
            >
              <ChevronDown className="size-4" aria-hidden />
            </Button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content
              className="z-[60] min-w-[12rem] rounded-md border border-border bg-card p-1 shadow-md"
              sideOffset={4}
              align="end"
            >
              {options.map((opt) => (
                <DropdownMenu.Item
                  key={opt.id}
                  disabled={opt.disabled}
                  className="flex cursor-pointer items-center gap-2 rounded-sm px-2 py-1.5 text-sm outline-none hover:bg-accent focus:bg-accent data-[disabled]:pointer-events-none data-[disabled]:opacity-50"
                  onSelect={() => onValueChange(opt.id)}
                >
                  <Check
                    className={cn(
                      "size-4 shrink-0",
                      opt.id === value ? "opacity-100" : "opacity-0",
                    )}
                    aria-hidden
                  />
                  {opt.label}
                </DropdownMenu.Item>
              ))}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      ) : null}
    </div>
  );
}
