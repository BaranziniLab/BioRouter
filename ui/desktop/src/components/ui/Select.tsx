import React from 'react';
import ReactSelect from 'react-select';

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
        control: ({ isFocused }) =>
          `border ${isFocused ? 'border-border-subtle' : 'border-border-subtle'} focus:border-border-subtle hover:border-border-subtle rounded-md w-full px-4 py-2 text-sm text-text-muted hover:cursor-pointer`,
        // text-sm on the menu pins the dropdown font to 14px. react-select sets
        // `fontSize: inherit` on options even when `unstyled`, and Emotion injects
        // that rule after Tailwind so the `text-sm` we put on the option itself
        // gets overridden — the option ends up inheriting from <body> at 16px,
        // which is visibly larger than the 14px control. Setting text-sm on the
        // menu makes the inherit chain resolve to 14px.
        menu: () =>
          'mt-1 bg-background-default border border-border-subtle rounded-md text-text-muted text-sm shadow-lg select__menu z-[9999] absolute',
        menuList: () => 'max-h-60 overflow-y-auto py-1',
        option: ({ isFocused, isSelected, isDisabled }) => {
          let classes = 'py-2 px-4 cursor-pointer';

          if (isDisabled) {
            classes += ' opacity-50 cursor-not-allowed pointer-events-none';
          } else if (isSelected) {
            classes += ' bg-background-accent text-text-on-accent pointer-events-auto';
          } else if (isFocused) {
            classes += ' bg-background-muted text-text-default pointer-events-auto';
          } else {
            classes += ' text-text-default hover:bg-background-muted pointer-events-auto';
          }

          return classes;
        },
      }}
      menuShouldBlockScroll={false}
      menuShouldScrollIntoView={false}
      tabSelectsValue={true}
      openMenuOnFocus={false}
      styles={{
        menu: (base) => ({
          ...base,
          pointerEvents: 'auto',
          zIndex: 9999,
        }),
        menuList: (base) => ({
          ...base,
          maxHeight: '240px',
          overflowY: 'auto',
          pointerEvents: 'auto',
        }),
        option: (base) => ({
          ...base,
          pointerEvents: 'auto',
          cursor: 'pointer',
        }),
      }}
    />
  );
};
