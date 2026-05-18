import * as DropdownMenu from "@radix-ui/react-dropdown-menu";
import { ChevronDown } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export type SplitButtonOption = {
  id: number;
  label: string;
};

type SplitButtonProps = {
  label: string;
  options: SplitButtonOption[];
  disabled?: boolean;
  loading?: boolean;
  onPrimaryClick: () => void;
  onSelect: (id: number) => void;
};

export function SplitButton({
  label,
  options,
  disabled,
  loading,
  onPrimaryClick,
  onSelect,
}: SplitButtonProps) {
  const canMenu = options.length > 1;

  return (
    <div className="inline-flex rounded-md">
      <Button
        type="button"
        variant="secondary"
        size="sm"
        disabled={disabled || loading}
        className={cn(canMenu && "rounded-r-none border-r border-border")}
        onClick={onPrimaryClick}
      >
        {loading ? "Loading…" : label}
      </Button>
      {canMenu && (
        <DropdownMenu.Root>
          <DropdownMenu.Trigger asChild>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={disabled || loading}
              className="rounded-l-none px-2"
              aria-label="Choose provider"
            >
              <ChevronDown className="size-4" />
            </Button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content
              className="z-[60] min-w-[10rem] rounded-md border border-border bg-card p-1 shadow-md"
              sideOffset={4}
              align="end"
            >
              {options.map((opt) => (
                <DropdownMenu.Item
                  key={opt.id}
                  className="cursor-pointer rounded-sm px-2 py-1.5 text-sm outline-none hover:bg-accent focus:bg-accent"
                  onSelect={() => onSelect(opt.id)}
                >
                  {opt.label}
                </DropdownMenu.Item>
              ))}
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      )}
    </div>
  );
}
