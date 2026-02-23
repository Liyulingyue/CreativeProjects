interface VocabCardProps {
  word: string
  phonetic: string | null
  part_of_speech: string | null
  definition: string
  index?: number
  showChinese?: boolean
  showEnglish?: boolean
}

export default function VocabCard({ 
  word, 
  phonetic, 
  part_of_speech, 
  definition, 
  index,
  showChinese = true,
  showEnglish = true
}: VocabCardProps) {
  return (
    <div className={`vocab-card ${!showChinese ? 'hide-chinese' : ''} ${!showEnglish ? 'hide-english' : ''}`}>
      {index !== undefined && (
        <div className="vocab-index">{index + 1}</div>
      )}
      <div className="vocab-word">{word}</div>
      <div className="vocab-phonetic">{phonetic || ''}</div>
      <div className="vocab-pos">{part_of_speech || ''}</div>
      <div className="vocab-definition">{definition}</div>
    </div>
  )
}
