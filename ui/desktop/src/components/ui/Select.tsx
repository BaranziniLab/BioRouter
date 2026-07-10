import React from 'react';
import ReactSelect from 'react-select';

/**
 * The shared select (design.md §4.8).
 *
 * The trigger is visually identical to <Input>: same height, radius, border and
 * focus treatment. The menu is the shared popover surface.
 *
 * Layering: every call site lives inside a modal, and the menu is NOT portalled —
 * react-select renders it as an absolutely-positioned sibling. It therefore needs
 * to out-rank the modal surface (--z-modal, 400) without out-ranking a toast
 * (--z-toast, 600). It previously used z-[9999], which painted over the dialog's
 * own close button.
 *
 * `unstyled` strips react-select's Emotion base styles, but Emotion still injects
 * `fontSize: inherit` on options AFTER Tailwind's layer. Pinning `text-sm` on the
 * menu makes the inherit chain resolve to 14px instead of the <body>'s 16px.
 *
 * D-08 selected a full migration to Radix Select + Command. That is staged
 * separately: three of the four call sites are plain selects, but the model
 * picker in SwitchModelModal is a typeahead combobox (onInputChange/inputValue)
 * which Radix Select cannot express without `cmdk`.
 */
const MENU_Z = 500; // --z-modal-dropdown

export const Select = (props: React.ComponentProps<typeof ReactSelect>) => {
  return (
    <ReactSelect
      {...props}
      unstyled
      isSearchable={props.isSearchable !== false}
      closeMenuOnSelect={props.closeMenuOnSelect !== false}
      blurInputOnSelect={props.blurInputOnSelect !== false}
      classNames={{
        container: () => 'w-full cursor-pointer relative',
        indicatorSeparator: () => 'h-0',
        control: ({ isFocused, isDisabled }) =>
          [
            'flex h-9 w-full items-center rounded-md border bg-background-default px-3',
            'text-sm text-text-default transition-colors',
            isDisabled
              ? 'cursor-not-allowed opacity-50'
              : 'cursor-pointer hover:border-border-strong',
            isFocused
              ? 'border-[var(--border-focus)] bg-[var(--background-focus)]'
              : 'border-border-input',
          ].join(' '),
        placeholder: () => 'text-text-muted',
        singleValue: () => 'text-text-default',
        input: () => 'text-text-default',
        dropdownIndicator: () =>
          'text-text-muted transition-transform duration-[var(--motion-fast)]',
        menu: () =>
          'biorouter-popover-surface mt-1.5 bg-background-default rounded-2xl text-sm select__menu absolute',
        menuList: () => 'max-h-60 overflow-y-auto p-1',
        groupHeading: () =>
          'px-3 pt-2 pb-1.5 text-[11px] font-medium uppercase tracking-wider text-text-muted',
        noOptionsMessage: () => 'px-3 py-2 text-sm text-text-muted',
        option: ({ isFocused, isSelected, isDisabled }) => {
          const base = 'flex h-8 items-center rounded-md px-3 text-sm cursor-pointer';
          if (isDisabled) return `${base} opacity-50 cursor-not-allowed pointer-events-none`;
          if (isSelected) {
            // A selected row is emphasised, not inverted. It used to fill with
            // bg-background-accent — a near-black block in a menu of neutral rows.
            return `${base} bg-background-medium font-medium text-text-default pointer-events-auto`;
          }
          if (isFocused) return `${base} bg-background-muted text-text-default pointer-events-auto`;
          return `${base} text-text-default hover:bg-background-muted pointer-events-auto`;
        },
      }}
      menuShouldBlockScroll={false}
      menuShouldScrollIntoView={false}
      tabSelectsValue={true}
      openMenuOnFocus={false}
      styles={{
        menu: (base) => ({ ...base, pointerEvents: 'auto', zIndex: MENU_Z }),
        menuList: (base) => ({
          ...base,
          maxHeight: '240px',
          overflowY: 'auto',
          pointerEvents: 'auto',
        }),
        option: (base) => ({ ...base, pointerEvents: 'auto', cursor: 'pointer' }),
      }}
    />
  );
};
