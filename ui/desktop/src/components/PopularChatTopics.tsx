import React from 'react';
import { Activity, Users, Pill, HeartPulse } from 'lucide-react';

interface PopularChatTopicsProps {
  append: (text: string) => void;
}

interface ChatTopic {
  id: string;
  icon: React.ReactNode;
  description: string;
  prompt: string;
}

const POPULAR_TOPICS: ChatTopic[] = [
  {
    id: 'diabetes-statin-therapy',
    icon: <Activity className="w-5 h-5" />,
    description: 'How many patients with type 2 diabetes are currently receiving statin therapy',
    prompt: 'How many patients with type 2 diabetes are currently receiving statin therapy',
  },
  {
    id: 'ms-prevalence',
    icon: <Users className="w-5 h-5" />,
    description: 'What is the prevalence of multiple sclerosis across different age groups in our EHR data',
    prompt: 'What is the prevalence of multiple sclerosis across different age groups in our EHR data',
  },
  {
    id: 'breast-cancer-treatment',
    icon: <Pill className="w-5 h-5" />,
    description: 'How many active breast cancer patients are receiving systemic treatment today',
    prompt: 'How many active breast cancer patients are receiving systemic treatment today',
  },
  {
    id: 'rheumatoid-biologics',
    icon: <HeartPulse className="w-5 h-5" />,
    description: 'Which patients with rheumatoid arthritis are being treated with biologic medications',
    prompt: 'Which patients with rheumatoid arthritis are being treated with biologic medications',
  },
];

export default function PopularChatTopics({ append }: PopularChatTopicsProps) {
  const handleTopicClick = (prompt: string) => {
    append(prompt);
  };

  return (
    <div className="absolute bottom-0 left-0 p-6 max-w-md">
      <h3 className="text-text-muted text-sm mb-1">Popular chat topics</h3>
      <div className="space-y-1">
        {POPULAR_TOPICS.map((topic) => (
          <div
            key={topic.id}
            className="flex items-center justify-between py-1.5 hover:bg-bgSubtle rounded-md cursor-pointer transition-colors"
            onClick={() => handleTopicClick(topic.prompt)}
          >
            <div className="flex items-center gap-3 flex-1 min-w-0">
              <div className="flex-shrink-0 text-text-muted">{topic.icon}</div>
              <div className="flex-1 min-w-0">
                <p className="text-text-default text-sm leading-tight">{topic.description}</p>
              </div>
            </div>
            <div className="flex-shrink-0 ml-4">
              <button
                className="text-sm text-text-muted hover:text-text-default transition-colors cursor-pointer"
                onClick={(e) => {
                  e.stopPropagation();
                  handleTopicClick(topic.prompt);
                }}
              >
                Start
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
