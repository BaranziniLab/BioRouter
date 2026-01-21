import { useState } from 'react';
import { useTextAnimator } from '../../hooks/use-text-animator';

interface GreetingProps {
  className?: string;
  forceRefresh?: boolean;
}

export function Greeting({
  className = 'mt-1 text-4xl font-light animate-in fade-in duration-300',
  forceRefresh = false,
}: GreetingProps) {
  const prefixes = ['Hello!'];
  const messages = [
    'What insights will your data reveal today?',
    'Which connections in the knowledge graph will lead to better care?',
    'What patient story will you uncover in the EHR today?',
    "Which patterns will the knowledge graph unlock for tomorrow's treatments?",
    'What unanswered question in the EHR can we tackle next?',
    "How will today's data bring us closer to a new breakthrough?",
    'Which patient trends are waiting to be discovered in the EHR?',
    'What surprising links might the knowledge graph reveal today?',
    "Which treatment paths can we refine from today's data?",
    'How will your next query shape patient outcomes?',
    'Which health discovery is hidden in your data today?',
    'What clinical journey will your analysis improve today?',
    'What relationships in the data will bring us closer to a cure?',
    'What question will your data answer next?',
    'Which medical mystery might the knowledge graph help solve today?',
  ];

  // Using lazy initializer to generate random greeting on each component instance
  const greeting = useState(() => {
    const randomPrefixIndex = Math.floor(Math.random() * prefixes.length);
    const randomMessageIndex = Math.floor(Math.random() * messages.length);

    return {
      prefix: prefixes[randomPrefixIndex],
      message: messages[randomMessageIndex],
    };
  })[0];

  const messageRef = useTextAnimator({ text: greeting.message });

  return (
    <h1 className={className} key={forceRefresh ? Date.now() : undefined}>
      {/* <span>{greeting.prefix}</span> */}
      <span ref={messageRef}>{greeting.message}</span>
    </h1>
  );
}
